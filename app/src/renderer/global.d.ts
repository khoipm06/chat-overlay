import type { AgentOverlayApi, MarkupBridgeApi } from '../preload/preload';

declare global {
  interface Window {
    agentOverlay: AgentOverlayApi;
    markupBridge: MarkupBridgeApi;
  }
}

export {};
