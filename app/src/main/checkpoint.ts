import { execFile } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import type { GitGlance } from '../shared/types';

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

    return {
      restored: true,
      message: `Workspace & chat restored (${treeSha.slice(0, 7)})`,
    };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    return { restored: false, message: `Chat reverted; file restore failed: ${msg}` };
  } finally {
    await fs.unlink(tmpIndex).catch(() => {});
  }
}
