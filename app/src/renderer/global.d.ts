import type { AgentOverlayApi } from '../preload/preload';
import type { MarkupAnnotation } from '../shared/types';

declare global {
  interface Window {
    agentOverlay: AgentOverlayApi;
    markupBridge: {
      onInit: (
        cb: (payload: {
          mode: 'annotate' | 'agent';
          screenshotDataUrl?: string;
          annotations?: MarkupAnnotation[];
        }) => void,
      ) => () => void;
      submitImage: (dataUrl: string) => void;
      cancel: () => void;
      setClickThrough: (ignore: boolean) => void;
    };
  }
}

export {};
