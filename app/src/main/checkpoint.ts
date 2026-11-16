import { execFile } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);
const GIT_TIMEOUT_MS = 15_000;

export interface CheckpointSnapshot {
  id: string;
  isGit: boolean;
  treeSha: string | null;
  createdAt: number;
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

/**
 * Captures the working-tree state strictly under `workspaceDir` using an isolated
 * temporary index file so the user's real `.git/index` and commit history stay clean.
 */
export async function captureWorkspaceCheckpoint(workspaceDir: string): Promise<CheckpointSnapshot> {
  const id = `ckpt_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`;
  const gitRoot = await resolveGitRoot(workspaceDir);
  if (!gitRoot) {
    return { id, isGit: false, treeSha: null, createdAt: Date.now() };
  }

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

    return {
      id,
      isGit: true,
      treeSha: stdout.trim() || null,
      createdAt: Date.now(),
    };
  } catch {
    return { id, isGit: false, treeSha: null, createdAt: Date.now() };
  } finally {
    await fs.unlink(tmpIndex).catch(() => {});
  }
}

/**
 * Restores working-tree files strictly inside `workspaceDir` back to the exact state
 * recorded in `treeSha` using `GIT_INDEX_FILE` so neither the user's real `.git/index`
 * nor files outside `workspaceDir` are modified.
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
    const env = { ...process.env, GIT_INDEX_FILE: tmpIndex };

    if (relScope === '.') {
      await execFileAsync('git', ['-C', gitRoot, 'read-tree', '--reset', '-u', treeSha], {
        env,
        timeout: GIT_TIMEOUT_MS,
      });
    } else {
      // Populate isolated tmpIndex at treeSha, then remove tracked files in relScope absent from treeSha
      // and check out ONLY relScope paths from treeSha without touching any parent/sibling directory.
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

    // Remove only untracked non-ignored files inside relScope not present in treeSha
    const { stdout: untrackedOut } = await execFileAsync(
      'git',
      ['-C', gitRoot, 'ls-files', '--others', '--exclude-standard', '-z', '--', relScope],
      { env, timeout: GIT_TIMEOUT_MS },
    );
    const scopeAbs = path.resolve(workspaceDir);
    for (const rel of untrackedOut.split('\0').filter(Boolean)) {
      const abs = path.resolve(gitRoot, rel);
      if (abs === scopeAbs || abs.startsWith(scopeAbs + path.sep)) {
        await fs.rm(abs, { recursive: true, force: true }).catch(() => {});
      }
    }

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
