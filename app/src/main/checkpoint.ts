import { execFile } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import type { GitDiffHunk, GitDiffLine, GitDiffReport, GitFileDiff, GitGlance } from '../shared/types';

const execFileAsync = promisify(execFile);
const GIT_TIMEOUT_MS = 15_000;

export interface CheckpointSnapshot {
  id: string;
  isGit: boolean;
  workspaceDir: string;
  treeSha: string | null;
  headSha: string | null;
  createdAt: number;
  error?: string;
}

async function resolveGitRoot(cwd: string): Promise<string | null> {
  try {
    const { stdout } = await execFileAsync('git', ['rev-parse', '--show-toplevel'], {
      cwd,
      timeout: GIT_TIMEOUT_MS,
    });
    const root = stdout.trim();
    return root ? path.resolve(root) : null;
  } catch {
    return null;
  }
}

async function resolveHeadSha(gitRoot: string): Promise<string | null> {
  try {
    const { stdout } = await execFileAsync('git', ['-C', gitRoot, 'rev-parse', 'HEAD'], {
      timeout: GIT_TIMEOUT_MS,
    });
    return stdout.trim() || null;
  } catch {
    return null;
  }
}

/**
 * Returns a lightweight git status/diff summary for the narrow overlay subheader.
 */
export async function getGitGlance(workspaceDir: string): Promise<GitGlance> {
  const empty: GitGlance = {
    isRepo: false,
    branch: '',
    changedFiles: [],
    stagedCount: 0,
    modifiedCount: 0,
    insertions: 0,
    deletions: 0,
  };
  const gitRoot = await resolveGitRoot(workspaceDir);
  if (!gitRoot) return empty;

  const relScope = path.relative(gitRoot, path.resolve(workspaceDir)) || '.';
  try {
    let branch = '';
    try {
      const { stdout } = await execFileAsync(
        'git',
        ['-C', gitRoot, 'symbolic-ref', '--short', 'HEAD'],
        { timeout: 3000 },
      );
      branch = stdout.trim();
    } catch {
      const head = await resolveHeadSha(gitRoot);
      branch = head ? head.slice(0, 7) : 'HEAD';
    }

    const { stdout: porcelain } = await execFileAsync(
      'git',
      ['-C', gitRoot, 'status', '--porcelain', '--', relScope],
      { timeout: 4000 },
    );
    const changedFiles: string[] = [];
    let stagedCount = 0;
    let modifiedCount = 0;
    for (const line of porcelain.split('\n')) {
      if (!line.trim()) continue;
      const x = line[0];
      const y = line[1];
      if (x !== ' ' && x !== '?') stagedCount++;
      if (y !== ' ' || x === '?') modifiedCount++;
      const filePart = line.slice(3).trim();
      if (filePart) changedFiles.push(filePart);
    }

    let insertions = 0;
    let deletions = 0;
    try {
      const { stdout: numstat } = await execFileAsync(
        'git',
        ['-C', gitRoot, 'diff', '--numstat', 'HEAD', '--', relScope],
        { timeout: 4000 },
      );
      for (const row of numstat.split('\n')) {
        const parts = row.trim().split(/\s+/);
        if (parts.length >= 2) {
          const ins = Number(parts[0]);
          const del = Number(parts[1]);
          if (!Number.isNaN(ins)) insertions += ins;
          if (!Number.isNaN(del)) deletions += del;
        }
      }
    } catch {
      // Unborn HEAD; ignore numstat
    }

    return {
      isRepo: true,
      branch,
      changedFiles: changedFiles.slice(0, 60),
      stagedCount,
      modifiedCount,
      insertions,
      deletions,
    };
  } catch {
    return empty;
  }
}

/**
 * Captures the working-tree state under `workspaceDir` into a GC-pinned Git tree object
 * using an isolated `GIT_INDEX_FILE` so the user's real `.git/index` is untouched.
 */
export async function captureWorkspaceCheckpoint(workspaceDir: string): Promise<CheckpointSnapshot> {
  const id = `ckpt_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`;
  const gitRoot = await resolveGitRoot(workspaceDir);
  if (!gitRoot) {
    return { id, isGit: false, workspaceDir, treeSha: null, headSha: null, createdAt: Date.now() };
  }

  const headSha = await resolveHeadSha(gitRoot);
  const tmpIndex = path.join(os.tmpdir(), `chat-overlay-idx-${id}`);
  try {
    const env = { ...process.env, GIT_INDEX_FILE: tmpIndex };
    await execFileAsync('git', ['-C', gitRoot, 'read-tree', 'HEAD'], {
      env,
      timeout: GIT_TIMEOUT_MS,
    }).catch(() => {});

    const relScope = path.relative(gitRoot, path.resolve(workspaceDir)) || '.';
    await execFileAsync('git', ['-C', gitRoot, 'add', '-A', '--', relScope], {
      env,
      timeout: GIT_TIMEOUT_MS,
    });

    const { stdout } = await execFileAsync('git', ['-C', gitRoot, 'write-tree'], {
      env,
      timeout: GIT_TIMEOUT_MS,
    });

    const treeSha = stdout.trim() || null;
    if (treeSha) {
      // Pin loose tree object under a hidden ref so `git gc` / `git prune` cannot delete it
      await execFileAsync(
        'git',
        ['-C', gitRoot, 'update-ref', `refs/chat-overlay/${id}`, treeSha],
        { timeout: 3000 },
      ).catch(() => {});
    }

    return {
      id,
      isGit: true,
      workspaceDir,
      treeSha,
      headSha,
      createdAt: Date.now(),
    };
  } catch (err) {
    return {
      id,
      isGit: true,
      workspaceDir,
      treeSha: null,
      headSha,
      createdAt: Date.now(),
      error: err instanceof Error ? err.message : String(err),
    };
  } finally {
    await fs.unlink(tmpIndex).catch(() => {});
  }
}

/**
 * Restores working-tree files strictly inside `workspaceDir` back to the exact state
 * recorded in `treeSha`, cleaning untracked files and nested empty directories without
 * mutating the user's real `.git/index` or files outside `workspaceDir`.
 */
export async function restoreWorkspaceCheckpoint(
  workspaceDir: string,
  treeSha: string | null,
): Promise<{ restored: boolean; message: string }> {
  if (!treeSha) {
    return { restored: false, message: 'Chat reverted (no git file checkpoint recorded)' };
  }
  const gitRoot = await resolveGitRoot(workspaceDir);
  if (!gitRoot) {
    return { restored: false, message: 'Chat reverted (workspace is not a git repo)' };
  }

  const relScope = path.relative(gitRoot, path.resolve(workspaceDir)) || '.';
  const tmpIndex = path.join(os.tmpdir(), `chat-overlay-restore-${Date.now()}`);
  try {
    // Verify tree object still exists before mutating working tree
    await execFileAsync('git', ['-C', gitRoot, 'cat-file', '-e', `${treeSha}^{tree}`], {
      timeout: 3000,
    });

    const env = { ...process.env, GIT_INDEX_FILE: tmpIndex };

    if (relScope === '.') {
      await execFileAsync('git', ['-C', gitRoot, 'read-tree', '--reset', '-u', treeSha], {
        env,
        timeout: GIT_TIMEOUT_MS,
      });
    } else {
      await execFileAsync('git', ['-C', gitRoot, 'read-tree', treeSha], {
        env,
        timeout: GIT_TIMEOUT_MS,
      });

      const { stdout: targetList } = await execFileAsync(
        'git',
        ['-C', gitRoot, 'ls-tree', '-r', '--name-only', '-z', treeSha, '--', relScope],
        { timeout: GIT_TIMEOUT_MS },
      );
      const keepSet = new Set(targetList.split('\0').filter(Boolean));

      const { stdout: currentTracked } = await execFileAsync(
        'git',
        ['-C', gitRoot, 'ls-files', '-z', '--', relScope],
        { timeout: GIT_TIMEOUT_MS },
      );
      const scopeAbs = path.resolve(workspaceDir);
      for (const rel of currentTracked.split('\0').filter(Boolean)) {
        if (!keepSet.has(rel)) {
          const abs = path.resolve(gitRoot, rel);
          if (abs === scopeAbs || abs.startsWith(scopeAbs + path.sep)) {
            await fs.rm(abs, { force: true }).catch(() => {});
          }
        }
      }

      if (keepSet.size > 0) {
        await execFileAsync(
          'git',
          ['-C', gitRoot, 'checkout', treeSha, '--', relScope],
          { env, timeout: GIT_TIMEOUT_MS },
        );
      }
    }

    // Remove untracked non-ignored files AND empty subdirectories strictly inside relScope
    await execFileAsync('git', ['-C', gitRoot, 'clean', '-f', '-d', '--', relScope], {
      env,
      timeout: GIT_TIMEOUT_MS,
    });

    return { restored: true, message: `Workspace & chat restored (${treeSha.slice(0, 7)})` };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    return { restored: false, message: `Chat reverted; file restore failed: ${msg}` };
  } finally {
    await fs.unlink(tmpIndex).catch(() => {});
  }
}

export function parseUnifiedDiffOutput(rawDiff: string, maxLinesPerFile = 260): GitFileDiff[] {
  const files: GitFileDiff[] = [];
  if (!rawDiff.trim()) return files;

  const chunks = rawDiff.split(/^diff --git /m).filter(Boolean);
  for (const chunk of chunks) {
    const lines = chunk.split('\n');
    const firstLine = lines[0] || '';
    const match = firstLine.match(/^a\/(.+?)\s+b\/(.+)$/);
    const filePath = (match?.[2] || match?.[1] || 'unknown').trim();

    let status: GitFileDiff['status'] = 'modified';
    let additions = 0;
    let deletions = 0;
    const hunks: GitDiffHunk[] = [];
    let activeHunk: GitDiffHunk | null = null;
    let totalDiffLines = 0;
    let truncated = false;

    for (let i = 1; i < lines.length; i++) {
      const line = lines[i];
      if (line.startsWith('new file mode')) {
        status = 'added';
        continue;
      }
      if (line.startsWith('deleted file mode')) {
        status = 'deleted';
        continue;
      }
      if (line.startsWith('index ') || line.startsWith('--- ') || line.startsWith('+++ ')) {
        continue;
      }
      if (line.startsWith('@@')) {
        activeHunk = { header: line, lines: [] };
        hunks.push(activeHunk);
        continue;
      }
      if (!activeHunk) continue;
      if (i === lines.length - 1 && line === '') continue;
      if (line === '\\ No newline at end of file') continue;

      if (totalDiffLines >= maxLinesPerFile) {
        truncated = true;
        if (line.startsWith('+') && !line.startsWith('+++')) additions++;
        else if (line.startsWith('-') && !line.startsWith('---')) deletions++;
        continue;
      }

      let kind: GitDiffLine['kind'] = 'ctx';
      if (line.startsWith('+')) {
        kind = 'add';
        additions++;
      } else if (line.startsWith('-')) {
        kind = 'del';
        deletions++;
      }
      activeHunk.lines.push({ kind, text: line });
      totalDiffLines++;
    }

    files.push({ filePath, status, additions, deletions, hunks, truncated });
  }
  return files;
}

export async function getGitDiffReport(
  workspaceDir: string,
  targetFile?: string,
): Promise<GitDiffReport> {
  const glance = await getGitGlance(workspaceDir);
  if (!glance.isRepo) {
    return { isRepo: false, branch: '', files: [] };
  }
  const gitRoot = await resolveGitRoot(workspaceDir);
  if (!gitRoot) {
    return { isRepo: false, branch: '', files: [] };
  }

  const relScope = path.relative(gitRoot, path.resolve(workspaceDir)) || '.';
  const scopeArg = targetFile
    ? path.isAbsolute(targetFile)
      ? path.relative(gitRoot, targetFile)
      : path.relative(gitRoot, path.resolve(workspaceDir, targetFile)) || targetFile
    : relScope;

  let rawDiff = '';
  try {
    const { stdout } = await execFileAsync(
      'git',
      ['-C', gitRoot, 'diff', 'HEAD', '--unified=3', '--', scopeArg],
      { timeout: GIT_TIMEOUT_MS, maxBuffer: 4 * 1024 * 1024 },
    );
    rawDiff = stdout;
  } catch {
    try {
      const { stdout } = await execFileAsync(
        'git',
        ['-C', gitRoot, 'diff', '--unified=3', '--', scopeArg],
        { timeout: GIT_TIMEOUT_MS, maxBuffer: 4 * 1024 * 1024 },
      );
      rawDiff = stdout;
    } catch {
      rawDiff = '';
    }
  }

  const files = parseUnifiedDiffOutput(rawDiff);
  const trackedSet = new Set(files.map((f) => f.filePath));

  // Include untracked files within scope
  try {
    const { stdout: untrackedOut } = await execFileAsync(
      'git',
      ['-C', gitRoot, 'ls-files', '--others', '--exclude-standard', '-z', '--', scopeArg],
      { timeout: 4000 },
    );
    for (const rel of untrackedOut.split('\0').filter(Boolean).slice(0, 20)) {
      if (trackedSet.has(rel)) continue;
      const abs = path.join(gitRoot, rel);
      try {
        const buf = await fs.readFile(abs);
        if (buf.includes(0)) {
          files.push({
            filePath: rel,
            status: 'untracked',
            additions: 0,
            deletions: 0,
            hunks: [{ header: '@@ untracked binary file @@', lines: [] }],
          });
          continue;
        }
        const textLines = buf.toString('utf8').split('\n');
        const capped = textLines.slice(0, 120);
        files.push({
          filePath: rel,
          status: 'untracked',
          additions: textLines.length,
          deletions: 0,
          hunks: [
            {
              header: `@@ -0,0 +1,${capped.length} @@`,
              lines: capped.map((l) => ({ kind: 'add', text: `+${l}` })),
            },
          ],
          truncated: textLines.length > capped.length,
        });
      } catch {
        // Ignore unreadable untracked file
      }
    }
  } catch {
    // Ignore ls-files failure
  }

  return { isRepo: true, branch: glance.branch, files };
}

export async function restoreGitFile(
  workspaceDir: string,
  filePath: string,
): Promise<{ restored: boolean; message: string }> {
  const gitRoot = await resolveGitRoot(workspaceDir);
  if (!gitRoot) {
    return { restored: false, message: 'Not inside a Git repository.' };
  }
  const scopeAbs = path.resolve(workspaceDir);
  const targetAbs = path.isAbsolute(filePath)
    ? path.resolve(filePath)
    : path.resolve(gitRoot, filePath);
  if (targetAbs !== gitRoot && !targetAbs.startsWith(gitRoot + path.sep)) {
    return { restored: false, message: 'Refusing to restore path outside repository root.' };
  }
  if (targetAbs !== scopeAbs && !targetAbs.startsWith(scopeAbs + path.sep)) {
    return { restored: false, message: 'Refusing to restore path outside workspace.' };
  }

  const relFromRoot = path.relative(gitRoot, targetAbs);
  try {
    const { stdout: untracked } = await execFileAsync(
      'git',
      ['-C', gitRoot, 'ls-files', '--others', '--exclude-standard', '--', relFromRoot],
      { timeout: 3000 },
    );
    if (untracked.trim()) {
      await fs.rm(targetAbs, { force: true });
      return { restored: true, message: `Removed untracked file ${path.basename(filePath)}` };
    }
    await execFileAsync('git', ['-C', gitRoot, 'reset', '-q', 'HEAD', '--', relFromRoot], {
      timeout: 3000,
    }).catch(() => {});
    await execFileAsync('git', ['-C', gitRoot, 'checkout', 'HEAD', '--', relFromRoot], {
      timeout: 5000,
    });
    return { restored: true, message: `Restored ${path.basename(filePath)} to HEAD` };
  } catch (err) {
    return {
      restored: false,
      message: `Could not restore ${path.basename(filePath)}: ${err instanceof Error ? err.message : String(err)}`,
    };
  }
}
