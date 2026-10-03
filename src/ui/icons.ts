const paths = {
  speaker: '<path d="m11 5-6 4H2v6h3l6 4V5Z"/><path d="M15 8a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14"/>',
  copy: '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V4a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h4"/>',
  pin: '<path d="m16 3 5 5-4 1-4 4 1 4-3 3-7-7 3-3 4 1 4-4 1-4Z"/><path d="m8 16-5 5"/>',
  close: '<path d="m6 6 12 12M6 18 18 6"/>',
  more: '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
  book: '<path d="M12 5v15M3 3c4 0 6 0 9 2 3-2 5-2 9-2v15c-4 0-6 0-9 2-3-2-5-2-9-2V3Z"/>',
  history: '<path d="M3 11a9 9 0 1 1 2 7M3 4v7h7M12 7v5l3 2"/>',
} as const;

export function icon(name: keyof typeof paths): string {
  return `<svg class="ui-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths[name]}</svg>`;
}

export function speakerMarkup(label: string, id = ""): string {
  return `<button ${id ? `id="${id}"` : ""} class="speaker-button" type="button" aria-label="${label}" title="${label}，再次点击停止" aria-pressed="false">${icon("speaker")}</button>`;
}
