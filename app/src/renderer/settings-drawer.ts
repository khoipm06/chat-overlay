import { HARNESS_LIST } from '../shared/types';
import type {
  AppSettings,
  DesktopCapabilities,
  DesktopControlMode,
  FollowUpBehavior,
  HarnessDiagnostic,
  HarnessKind,
  HeightPreset,
  PermissionPolicy,
  RevertMode,
  ScreenshotQuality,
  WidthPreset,
} from '../shared/types';
import { escapeHtmlText } from './tool-presenter';

type SettingsTab = 'overlay' | 'safety' | 'vision' | 'harnesses';

export function renderTabbedSettingsDrawer(opts: {
  container: HTMLElement;
  settings: AppSettings;
  onUpdateSettings: (patch: Partial<AppSettings>) => Promise<AppSettings>;
  onClose: () => void;
}): void {
  let activeTab: SettingsTab = 'overlay';
  let current = opts.settings;
  let diagnostics: HarnessDiagnostic[] = [];
  let diagnosticsLoading = true;
  let desktopCaps: DesktopCapabilities | null = null;
  const harnessAcpState = new Map<
    HarnessKind,
    { tested: boolean; ok: boolean; latencyMs?: number; message?: string }
  >();

  const render = () => {
    opts.container.innerHTML = '';

    const tabsBar = document.createElement('div');
    tabsBar.className = 'settings-tabs';
    const tabDefs: Array<{ id: SettingsTab; label: string; icon: string }> = [
      { id: 'overlay', label: 'Overlay', icon: 'codicon-layout-sidebar-right' },
      { id: 'safety', label: 'Safety', icon: 'codicon-shield' },
      { id: 'vision', label: 'Vision & Flow', icon: 'codicon-device-camera' },
      { id: 'harnesses', label: 'Harnesses', icon: 'codicon-plug' },
    ];
    for (const t of tabDefs) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = `settings-tab-btn${t.id === activeTab ? ' active' : ''}`;
      btn.innerHTML = `<i class="codicon ${t.icon}"></i><span>${t.label}</span>`;
      btn.addEventListener('click', () => {
        activeTab = t.id;
        render();
      });
      tabsBar.appendChild(btn);
    }

    const body = document.createElement('div');
    body.className = 'settings-tab-content';

    if (activeTab === 'overlay') {
      body.innerHTML = `
        <div class="field-group">
          <label>Screen Dock Edge</label>
          <select id="set-edge" class="field-select">
            <option value="right" ${current.edge === 'right' ? 'selected' : ''}>Right Edge</option>
            <option value="left" ${current.edge === 'left' ? 'selected' : ''}>Left Edge</option>
          </select>
        </div>
        <div class="field-group">
          <label>Sidebar Width Preset</label>
          <select id="set-width-preset" class="field-select">
            <option value="narrow_15" ${current.widthPreset === 'narrow_15' ? 'selected' : ''}>15% Screen Width (Compact)</option>
            <option value="balanced_20" ${current.widthPreset === 'balanced_20' ? 'selected' : ''}>20% Screen Width (Balanced)</option>
            <option value="wide_25" ${current.widthPreset === 'wide_25' ? 'selected' : ''}>25% Screen Width (Recommended)</option>
            <option value="custom" ${current.widthPreset === 'custom' ? 'selected' : ''}>Custom Dragged Size</option>
          </select>
        </div>
        <div class="field-group">
          <label>Sidebar Height Preset</label>
          <select id="set-height-preset" class="field-select">
            <option value="compact_72" ${current.heightPreset === 'compact_72' ? 'selected' : ''}>72% Screen Height (Compact Floating)</option>
            <option value="balanced_78" ${current.heightPreset === 'balanced_78' ? 'selected' : ''}>78% Screen Height (Balanced)</option>
            <option value="full" ${current.heightPreset === 'full' ? 'selected' : ''}>Full Screen Height</option>
            <option value="custom" ${current.heightPreset === 'custom' ? 'selected' : ''}>Custom Dragged Size</option>
          </select>
        </div>
        <div class="field-group">
          <label>Opacity (${Math.round((current.opacity ?? 1) * 100)}%)</label>
          <input id="set-opacity" type="range" min="70" max="100" value="${Math.round((current.opacity ?? 1) * 100)}" />
        </div>
        <div class="field-group">
          <label>Font Size (px)</label>
          <input id="set-font-size" type="number" min="11" max="18" class="field-input" value="${current.fontSize}" />
        </div>
        <label class="toggle-row">
          <span>Launch at system login (minimized to tray)</span>
          <input id="set-launch-login" type="checkbox" ${current.launchAtLogin ? 'checked' : ''} />
        </label>
        <label class="toggle-row">
          <span>Dim slightly when unfocused</span>
          <input id="set-dim-blur" type="checkbox" ${current.dimOnBlur ? 'checked' : ''} />
        </label>
        <label class="toggle-row">
          <span>Auto-hide when unfocused</span>
          <input id="set-auto-hide" type="checkbox" ${current.autoHideOnBlur ? 'checked' : ''} />
        </label>
        <label class="toggle-row">
          <span>Show Git branch &amp; diff glance</span>
          <input id="set-git-glance" type="checkbox" ${current.showGitGlance ? 'checked' : ''} />
        </label>
        <div class="settings-hint">
          Toggle Ghost: <code>Ctrl+Alt+G</code> · Clipboard Companion: <code>Ctrl+Alt+Q</code> · Model Picker: <code>Ctrl+.</code>
        </div>
      `;
    } else if (activeTab === 'safety') {
      body.innerHTML = `
        <div class="field-group">
          <label>Tool Permission Policy</label>
          <select id="set-perm-policy" class="field-select">
            <option value="auto_read" ${current.permissionPolicy === 'auto_read' ? 'selected' : ''}>Auto-approve Read-Only (Recommended)</option>
            <option value="auto_workspace" ${current.permissionPolicy === 'auto_workspace' ? 'selected' : ''}>Auto-approve Workspace Edits &amp; Reads</option>
            <option value="ask" ${current.permissionPolicy === 'ask' ? 'selected' : ''}>Ask before every tool</option>
            <option value="yolo" ${current.permissionPolicy === 'yolo' ? 'selected' : ''}>Auto-approve all tools</option>
          </select>
        </div>
        <div class="field-group">
          <label>Desktop Actions &amp; Computer Use Policy</label>
          <select id="set-desktop-mode" class="field-select">
            <option value="auto" ${current.desktopControlMode === 'auto' ? 'selected' : ''}>Auto-run immediately (no delay)</option>
            <option value="preview_auto" ${current.desktopControlMode === 'preview_auto' ? 'selected' : ''}>Visual Preview + Auto-Run (1.2s Panic Stop)</option>
            <option value="confirm" ${current.desktopControlMode === 'confirm' ? 'selected' : ''}>Confirm every UI / OS action (Recommended)</option>
            <option value="off" ${current.desktopControlMode === 'off' ? 'selected' : ''}>Disabled for AI prompts</option>
          </select>
        </div>
        <div class="field-group">
          <label>Computer Tool Actions (Desktop Automation)</label>
          <div class="toggle-row" style="margin-top:4px;">
            <span>Auto-approve Computer &amp; Desktop Actions</span>
            <input id="set-auto-desktop" type="checkbox" ${current.autoApproveDesktopActions ? 'checked' : ''} />
          </div>
          <div class="settings-hint">
            When enabled, computer actions execute automatically. If disabled, a 'Run' button appears for manual approval.
          </div>
        </div>
        <div class="diag-card" style="margin-top:4px;">
          <div class="diag-card-top">
            <strong>Desktop Drivers</strong>
            <span class="badge">${desktopCaps ? escapeHtmlText(desktopCaps.platform) : 'checking...'}</span>
          </div>
          <div class="diag-card-sub">${
            desktopCaps
              ? `Input: ${escapeHtmlText(desktopCaps.inputDriver || 'unavailable')} · Theme: ${desktopCaps.themeControl ? 'ready' : 'n/a'} · Audio: ${desktopCaps.audioControl ? 'ready' : 'n/a'}${desktopCaps.notes ? `<br>${escapeHtmlText(desktopCaps.notes)}` : ''}`
              : 'Detecting OS quick action & input drivers...'
          }</div>
        </div>
        <div class="field-group">
          <label>Revert Mode</label>
          <select id="set-revert-mode" class="field-select">
            <option value="workspace_and_chat" ${current.revertMode === 'workspace_and_chat' ? 'selected' : ''}>Workspace Files + Chat (Git Checkpoint)</option>
            <option value="chat_only" ${current.revertMode === 'chat_only' ? 'selected' : ''}>Chat History Only (Leave Files Untouched)</option>
          </select>
        </div>
        <div class="field-group">
          <label>Session-Trusted Tools</label>
          <button id="btn-clear-trusted" type="button" class="mini-action-btn">Clear Remembered Tool Approvals</button>
        </div>
      `;
      body.querySelector('#btn-clear-trusted')?.addEventListener('click', async (e) => {
        const b = e.currentTarget as HTMLButtonElement;
        await window.agentOverlay.clearSessionAllowedTools();
        b.textContent = 'Cleared ✓';
      });
    } else if (activeTab === 'vision') {
      body.innerHTML = `
        <div class="field-group">
          <label>Sending Follow-up While Running</label>
          <select id="set-followup" class="field-select">
            <option value="queue" ${current.followUpBehavior === 'queue' ? 'selected' : ''}>Queue until current turn finishes (Enter)</option>
            <option value="steer" ${current.followUpBehavior === 'steer' ? 'selected' : ''}>Steer immediately (Interrupt &amp; redirect)</option>
          </select>
        </div>
        <div class="field-group">
          <label>Screen Capture Quality</label>
          <select id="set-shot-quality" class="field-select">
            <option value="compressed" ${current.screenshotQuality === 'compressed' ? 'selected' : ''}>Fast JPEG (~200KB, max 1600px)</option>
            <option value="original" ${current.screenshotQuality === 'original' ? 'selected' : ''}>Lossless PNG (Full Retina/4K)</option>
          </select>
        </div>
        <div class="field-group">
          <label>Voice Input Language (Microphone STT)</label>
          <select id="set-speech-lang" class="field-select">
            <option value="vi-VN" ${current.speechLanguage === 'vi-VN' ? 'selected' : ''}>Tiếng Việt (vi-VN)</option>
            <option value="en-US" ${current.speechLanguage === 'en-US' ? 'selected' : ''}>English (en-US)</option>
            <option value="auto" ${current.speechLanguage === 'auto' ? 'selected' : ''}>Auto / Browser default</option>
          </select>
        </div>
        <div class="field-group">
          <label>Auto-clear Agent Screen Annotations (seconds, 0 = manual)</label>
          <input id="set-markup-clear" type="number" min="0" max="120" class="field-input" value="${current.markupAutoClearSeconds}" />
        </div>
        <div class="field-group">
          <label>Sleep idle background agent after (minutes, 0 = never)</label>
          <input id="set-idle-sleep" type="number" min="0" max="180" class="field-input" value="${current.idleSleepMinutes}" />
        </div>
        <label class="toggle-row">
          <span>Subtle audio chime on completion &amp; approvals</span>
          <input id="set-sound-cues" type="checkbox" ${current.soundCues ? 'checked' : ''} />
        </label>
        <label class="toggle-row">
          <span>Flash taskbar icon when approval needed</span>
          <input id="set-flash-attn" type="checkbox" ${current.flashOnAttention ? 'checked' : ''} />
        </label>
        <label class="toggle-row">
          <span>Desktop notification when done in background</span>
          <input id="set-bg-notify" type="checkbox" ${current.backgroundNotifications ? 'checked' : ''} />
        </label>
        <label class="toggle-row">
          <span>Always attach live @screen with prompts</span>
          <input id="set-pin-screen" type="checkbox" ${current.pinLiveScreen ? 'checked' : ''} />
        </label>
      `;
    } else {
      body.innerHTML = `
        <div class="field-group">
          <label>Default ACP Agent</label>
          <select id="set-harness" class="field-select">
            ${HARNESS_LIST.map(
              (h) =>
                `<option value="${h.id}" ${h.id === current.harness ? 'selected' : ''}>${escapeHtmlText(h.label)}</option>`,
            ).join('')}
          </select>
        </div>
        <div class="diag-list" id="diag-list"></div>
      `;

      const diagList = body.querySelector<HTMLElement>('#diag-list')!;
      if (diagnosticsLoading) {
        const loading = document.createElement('div');
        loading.className = 'diag-card-sub';
        loading.textContent = 'Checking installed ACP harnesses...';
        diagList.appendChild(loading);
      }
      for (const d of diagnostics) {
        const card = document.createElement('div');
        card.className = 'diag-card';
        const top = document.createElement('div');
        top.className = 'diag-card-top';
        const left = document.createElement('div');
        const testState = harnessAcpState.get(d.id);
        const dotCls = testState
          ? testState.ok
            ? 'ready'
            : 'error'
          : d.installed || d.viaRunner
            ? 'warming'
            : 'error';
        const dotTitle = testState
          ? testState.ok
            ? `ACP Verified (${testState.latencyMs}ms)`
            : `ACP Failed: ${testState.message}`
          : d.installed
            ? 'Installed (ACP unverified - click Test ACP)'
            : 'Not installed';
        left.innerHTML = `<span class="harness-dot ${dotCls}" title="${escapeHtmlText(dotTitle)}" style="display:inline-block;margin-right:6px;"></span><strong>${escapeHtmlText(d.label)}</strong>`;

        const btns = document.createElement('div');
        btns.style.cssText = 'display:inline-flex;align-items:center;gap:5px;';

        const pingBtn = document.createElement('button');
        pingBtn.type = 'button';
        pingBtn.className = 'mini-action-btn';
        pingBtn.textContent = testState ? (testState.ok ? `OK ${testState.latencyMs}ms` : 'Failed') : d.installed || d.viaRunner ? 'Test ACP' : 'Missing';
        pingBtn.disabled = !d.installed && !d.viaRunner;

        const detail = document.createElement('div');
        detail.className = 'diag-card-sub';
        detail.textContent = testState
          ? testState.ok
            ? `${testState.message || 'Ready'} · ${testState.latencyMs}ms`
            : testState.message || 'ACP handshake failed'
          : d.installed
            ? `${d.resolvedPath}${d.version ? ` (${d.version})` : ''} · Click 'Test ACP' to verify server`
            : d.viaRunner
              ? `Auto-run via ${d.resolvedPath}`
              : `${d.resolvedPath} not found on PATH`;

        pingBtn.addEventListener('click', async () => {
          pingBtn.disabled = true;
          pingBtn.textContent = 'Testing...';
          const dotEl = left.querySelector<HTMLElement>('.harness-dot');
          if (dotEl) dotEl.className = 'harness-dot warming';
          const res = await window.agentOverlay.testHarness(d.id);
          harnessAcpState.set(d.id, {
            tested: true,
            ok: res.ok,
            latencyMs: res.latencyMs,
            message: res.message,
          });
          pingBtn.disabled = false;
          pingBtn.textContent = res.ok ? `OK ${res.latencyMs}ms` : 'Failed';
          if (dotEl) {
            dotEl.className = `harness-dot ${res.ok ? 'ready' : 'error'}`;
            dotEl.setAttribute(
              'title',
              res.ok ? `ACP Verified (${res.latencyMs}ms)` : `ACP Failed: ${res.message}`,
            );
          }
          detail.textContent = res.ok
            ? `${res.message} · ${res.latencyMs}ms`
            : res.message;
        });

        if (!d.installed && d.npmPackage) {
          const installBtn = document.createElement('button');
          installBtn.type = 'button';
          installBtn.className = 'mini-action-btn primary';
          installBtn.textContent = 'Install';
          installBtn.addEventListener('click', async () => {
            installBtn.disabled = true;
            installBtn.textContent = 'Installing...';
            const res = await window.agentOverlay.installHarness(d.id);
            detail.textContent = res.message;
            if (res.ok) {
              diagnostics = await window.agentOverlay.inspectHarnesses();
              render();
            } else {
              installBtn.disabled = false;
              installBtn.textContent = 'Retry Install';
            }
          });
          btns.appendChild(installBtn);
        }

        btns.appendChild(pingBtn);
        top.append(left, btns);
        card.append(top, detail);
        diagList.appendChild(card);
      }
    }

    const footer = document.createElement('div');
    footer.className = 'settings-footer';
    const snapBtn = document.createElement('button');
    snapBtn.type = 'button';
    snapBtn.className = 'mini-action-btn';
    snapBtn.textContent = 'Snap Sidebar Layout';
    snapBtn.addEventListener('click', async () => {
      await window.agentOverlay.applyWindowLayout(
        current.widthPreset === 'custom' ? 'wide_25' : current.widthPreset,
        current.heightPreset === 'custom' ? 'balanced_78' : current.heightPreset,
      );
    });

    const saveBtn = document.createElement('button');
    saveBtn.type = 'button';
    saveBtn.className = 'perm-btn primary';
    saveBtn.textContent = 'Save & Apply';
    saveBtn.addEventListener('click', async () => {
      const patch: Partial<AppSettings> = {};
      if (activeTab === 'overlay') {
        patch.edge = (body.querySelector('#set-edge') as HTMLSelectElement).value as 'left' | 'right';
        patch.widthPreset = (body.querySelector('#set-width-preset') as HTMLSelectElement).value as WidthPreset;
        patch.heightPreset = (body.querySelector('#set-height-preset') as HTMLSelectElement).value as HeightPreset;
        patch.opacity = Number((body.querySelector('#set-opacity') as HTMLInputElement).value) / 100;
        patch.fontSize = Math.min(18, Math.max(11, Number((body.querySelector('#set-font-size') as HTMLInputElement).value) || 13));
        patch.launchAtLogin = (body.querySelector('#set-launch-login') as HTMLInputElement).checked;
        patch.dimOnBlur = (body.querySelector('#set-dim-blur') as HTMLInputElement).checked;
        patch.autoHideOnBlur = (body.querySelector('#set-auto-hide') as HTMLInputElement).checked;
        patch.showGitGlance = (body.querySelector('#set-git-glance') as HTMLInputElement).checked;
      } else if (activeTab === 'safety') {
        patch.permissionPolicy = (body.querySelector('#set-perm-policy') as HTMLSelectElement).value as PermissionPolicy;
        patch.desktopControlMode = (body.querySelector('#set-desktop-mode') as HTMLSelectElement).value as DesktopControlMode;
        patch.autoApprovePermissions = patch.permissionPolicy === 'yolo';
        patch.revertMode = (body.querySelector('#set-revert-mode') as HTMLSelectElement).value as RevertMode;
      } else if (activeTab === 'vision') {
        patch.followUpBehavior = (body.querySelector('#set-followup') as HTMLSelectElement).value as FollowUpBehavior;
        patch.screenshotQuality = (body.querySelector('#set-shot-quality') as HTMLSelectElement).value as ScreenshotQuality;
        patch.markupAutoClearSeconds = Math.max(0, Number((body.querySelector('#set-markup-clear') as HTMLInputElement).value) || 0);
        patch.idleSleepMinutes = Math.max(0, Number((body.querySelector('#set-idle-sleep') as HTMLInputElement).value) || 0);
        patch.soundCues = (body.querySelector('#set-sound-cues') as HTMLInputElement).checked;
        patch.flashOnAttention = (body.querySelector('#set-flash-attn') as HTMLInputElement).checked;
        patch.backgroundNotifications = (body.querySelector('#set-bg-notify') as HTMLInputElement).checked;
        patch.pinLiveScreen = (body.querySelector('#set-pin-screen') as HTMLInputElement).checked;
      } else {
        patch.harness = (body.querySelector('#set-harness') as HTMLSelectElement).value as HarnessKind;
      }
      current = await opts.onUpdateSettings(patch);
      opts.onClose();
    });

    footer.append(snapBtn, saveBtn);
    opts.container.append(tabsBar, body, footer);
  };

  render();
  void window.agentOverlay
    .inspectHarnesses()
    .then((res) => {
      diagnostics = res;
      diagnosticsLoading = false;
      if (activeTab === 'harnesses' && opts.container.isConnected) {
        render();
      }
      if (current.harness) {
        void window.agentOverlay.testHarness(current.harness).then((testRes) => {
          harnessAcpState.set(current.harness, {
            tested: true,
            ok: testRes.ok,
            latencyMs: testRes.latencyMs,
            message: testRes.message,
          });
          if (activeTab === 'harnesses' && opts.container.isConnected) {
            render();
          }
        });
      }
    })
    .catch(() => {
      diagnosticsLoading = false;
    });
  void window.agentOverlay
    .getDesktopCapabilities()
    .then((caps) => {
      desktopCaps = caps;
      if (activeTab === 'safety' && opts.container.isConnected) {
        render();
      }
    })
    .catch(() => {});
}
