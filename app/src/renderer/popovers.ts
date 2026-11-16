export interface PopoverEntry {
  id: string;
  title: string;
  detail?: string;
  icon?: string;
  selected?: boolean;
  disabled?: boolean;
  onSelect: () => void;
}

export class PopoverController {
  private layer: HTMLElement;
  private activePopover: HTMLElement | null = null;
  private activeAnchor: HTMLElement | null = null;
  private activeIndex = 0;
  private currentItems: PopoverEntry[] = [];
  private outsideHandler: ((e: MouseEvent) => void) | null = null;
  private keyHandler: ((e: KeyboardEvent) => void) | null = null;

  constructor(layer: HTMLElement) {
    this.layer = layer;
  }

  isOpen(): boolean {
    return this.activePopover !== null;
  }

  close(): void {
    if (this.outsideHandler) {
      document.removeEventListener('mousedown', this.outsideHandler, true);
      this.outsideHandler = null;
    }
    if (this.keyHandler) {
      document.removeEventListener('keydown', this.keyHandler, true);
      this.keyHandler = null;
    }
    this.layer.innerHTML = '';
    this.activePopover = null;
    this.activeAnchor = null;
    this.currentItems = [];
    this.activeIndex = 0;
  }

  openList(opts: {
    anchor: HTMLElement;
    placement: 'above' | 'below';
    searchable?: boolean;
    searchPlaceholder?: string;
    emptyText?: string;
    headerText?: string;
    items: PopoverEntry[];
    minWidth?: number;
  }): void {
    const wasSameAnchor = this.activeAnchor === opts.anchor;
    this.close();
    if (wasSameAnchor) return;

    const pop = document.createElement('div');
    pop.className = 'popover';
    if (opts.minWidth) pop.style.minWidth = `${opts.minWidth}px`;

    let searchInput: HTMLInputElement | null = null;

    if (opts.headerText) {
      const hdr = document.createElement('div');
      hdr.style.cssText =
        'padding:6px 10px 4px;font-size:11px;font-weight:600;color:var(--text-muted);border-bottom:1px solid var(--border);';
      hdr.textContent = opts.headerText;
      pop.appendChild(hdr);
    }

    if (opts.searchable) {
      const wrap = document.createElement('div');
      wrap.className = 'popover-search-wrap';
      searchInput = document.createElement('input');
      searchInput.type = 'text';
      searchInput.className = 'popover-search';
      searchInput.placeholder = opts.searchPlaceholder || 'Search...';
      wrap.appendChild(searchInput);
      pop.appendChild(wrap);
    }

    const listEl = document.createElement('div');
    listEl.className = 'popover-list';
    pop.appendChild(listEl);

    const renderFiltered = (query: string) => {
      const q = query.trim().toLowerCase();
      const terms = q.split(/\s+/).filter(Boolean);
      const filtered =
        terms.length === 0
          ? opts.items
          : opts.items.filter((item) => {
              const hay = `${item.title} ${item.detail || ''} ${item.id}`.toLowerCase();
              return terms.every((t) => hay.includes(t));
            });

      this.currentItems = filtered;
      const selIdx = filtered.findIndex((i) => i.selected);
      this.activeIndex = selIdx >= 0 ? selIdx : 0;
      listEl.innerHTML = '';

      if (filtered.length === 0) {
        const empty = document.createElement('div');
        empty.style.cssText = 'padding:12px;text-align:center;color:var(--text-muted);font-size:12px;';
        empty.textContent = opts.emptyText || 'No matches';
        listEl.appendChild(empty);
        return;
      }

      filtered.forEach((item, idx) => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = `popover-item${idx === this.activeIndex ? ' active' : ''}`;
        if (item.disabled) btn.disabled = true;

        const main = document.createElement('div');
        main.className = 'popover-item-main';
        if (item.icon) {
          const ic = document.createElement('i');
          ic.className = `codicon ${item.icon}`;
          main.appendChild(ic);
        }
        const textWrap = document.createElement('div');
        textWrap.style.minWidth = '0';
        const title = document.createElement('div');
        title.className = 'popover-item-title';
        title.textContent = item.title;
        textWrap.appendChild(title);
        if (item.detail) {
          const det = document.createElement('div');
          det.className = 'popover-item-detail';
          det.textContent = item.detail;
          textWrap.appendChild(det);
        }
        main.appendChild(textWrap);
        btn.appendChild(main);

        if (item.selected) {
          const chk = document.createElement('i');
          chk.className = 'codicon codicon-check';
          btn.appendChild(chk);
        }

        btn.addEventListener('click', () => {
          if (item.disabled) return;
          this.close();
          item.onSelect();
        });
        listEl.appendChild(btn);
      });
    };

    renderFiltered('');
    this.layer.appendChild(pop);
    this.activePopover = pop;
    this.activeAnchor = opts.anchor;
    this.positionPopover(pop, opts.anchor, opts.placement);

    if (searchInput) {
      searchInput.addEventListener('input', () => {
        renderFiltered(searchInput!.value);
      });
      setTimeout(() => searchInput?.focus(), 0);
    }

    this.keyHandler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        this.close();
        return;
      }
      if (this.currentItems.length === 0) return;
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        this.moveHighlight(listEl, 1);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        this.moveHighlight(listEl, -1);
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const chosen = this.currentItems[this.activeIndex];
        if (chosen && !chosen.disabled) {
          this.close();
          chosen.onSelect();
        }
      }
    };
    document.addEventListener('keydown', this.keyHandler, true);

    this.outsideHandler = (e: MouseEvent) => {
      const target = e.target as Node | null;
      if (!target) return;
      if (pop.contains(target) || opts.anchor.contains(target)) return;
      this.close();
    };
    document.addEventListener('mousedown', this.outsideHandler, true);
  }

  openAutocomplete(opts: {
    anchor: HTMLElement;
    items: PopoverEntry[];
  }): void {
    this.close();
    if (opts.items.length === 0) return;

    const pop = document.createElement('div');
    pop.className = 'popover';
    pop.style.minWidth = `${Math.max(260, opts.anchor.getBoundingClientRect().width)}px`;

    const listEl = document.createElement('div');
    listEl.className = 'popover-list';
    pop.appendChild(listEl);

    this.currentItems = opts.items;
    this.activeIndex = 0;

    opts.items.forEach((item, idx) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = `popover-item${idx === 0 ? ' active' : ''}`;
      const main = document.createElement('div');
      main.className = 'popover-item-main';
      if (item.icon) {
        const ic = document.createElement('i');
        ic.className = `codicon ${item.icon}`;
        main.appendChild(ic);
      }
      const textWrap = document.createElement('div');
      textWrap.style.minWidth = '0';
      const title = document.createElement('div');
      title.className = 'popover-item-title';
      title.textContent = item.title;
      textWrap.appendChild(title);
      if (item.detail) {
        const det = document.createElement('div');
        det.className = 'popover-item-detail';
        det.textContent = item.detail;
        textWrap.appendChild(det);
      }
      main.appendChild(textWrap);
      btn.appendChild(main);

      btn.addEventListener('mousedown', (e) => {
        e.preventDefault();
        this.close();
        item.onSelect();
      });
      listEl.appendChild(btn);
    });

    this.layer.appendChild(pop);
    this.activePopover = pop;
    this.activeAnchor = opts.anchor;
    this.positionPopover(pop, opts.anchor, 'above');

    this.outsideHandler = (e: MouseEvent) => {
      const target = e.target as Node | null;
      if (!target) return;
      if (pop.contains(target) || opts.anchor.contains(target)) return;
      this.close();
    };
    document.addEventListener('mousedown', this.outsideHandler, true);
  }

  handleAutocompleteKey(e: KeyboardEvent): boolean {
    if (!this.activePopover || this.currentItems.length === 0) return false;
    const listEl = this.activePopover.querySelector<HTMLElement>('.popover-list');
    if (!listEl) return false;

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      this.moveHighlight(listEl, 1);
      return true;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      this.moveHighlight(listEl, -1);
      return true;
    }
    if (e.key === 'Tab' || e.key === 'Enter') {
      e.preventDefault();
      const item = this.currentItems[this.activeIndex];
      this.close();
      item?.onSelect();
      return true;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      this.close();
      return true;
    }
    return false;
  }

  private moveHighlight(listEl: HTMLElement, delta: number): void {
    const total = this.currentItems.length;
    if (total === 0) return;
    this.activeIndex = (this.activeIndex + delta + total) % total;
    const buttons = listEl.querySelectorAll<HTMLElement>('.popover-item');
    buttons.forEach((b, idx) => {
      b.classList.toggle('active', idx === this.activeIndex);
      if (idx === this.activeIndex) {
        b.scrollIntoView({ block: 'nearest' });
      }
    });
  }

  private positionPopover(pop: HTMLElement, anchor: HTMLElement, placement: 'above' | 'below'): void {
    const shellRect = this.layer.getBoundingClientRect();
    const rect = anchor.getBoundingClientRect();
    const pw = pop.offsetWidth || 240;
    const ph = pop.offsetHeight || 200;

    let left = rect.left - shellRect.left;
    if (left + pw > shellRect.width - 8) {
      left = Math.max(8, shellRect.width - pw - 8);
    }
    if (left < 8) left = 8;

    let top: number;
    if (placement === 'above') {
      top = rect.top - shellRect.top - ph - 6;
      if (top < 8) top = rect.bottom - shellRect.top + 6;
    } else {
      top = rect.bottom - shellRect.top + 6;
      if (top + ph > shellRect.height - 8) {
        top = Math.max(8, rect.top - shellRect.top - ph - 6);
      }
    }

    pop.style.left = `${Math.round(left)}px`;
    pop.style.top = `${Math.round(top)}px`;
  }
}
