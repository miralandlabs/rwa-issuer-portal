import {
  getTheme,
  subscribeTheme,
  toggleTheme,
  themeToggleIcon,
  themeToggleLabel,
} from "../services/theme";

export function renderHeader(root: HTMLElement): void {
  root.innerHTML = `
    <header class="topbar">
      <div class="brand">
        <h1>RWA Issuer Portal</h1>
        <p>Off-chain system of record · x402 rwa-kyc-hook</p>
      </div>
      <div class="topbar-actions">
        <button
          type="button"
          class="btn btn-ghost theme-toggle"
          id="theme-toggle"
          aria-label="${themeToggleLabel(getTheme())}"
          title="${themeToggleLabel(getTheme())}"
        >
          <span class="theme-toggle-icon" aria-hidden="true">${themeToggleIcon(getTheme())}</span>
        </button>
      </div>
    </header>
  `;

  const btn = root.querySelector("#theme-toggle") as HTMLButtonElement;
  const icon = btn?.querySelector(".theme-toggle-icon");
  btn?.addEventListener("click", () => toggleTheme());
  subscribeTheme(() => {
    const mode = getTheme();
    btn?.setAttribute("aria-label", themeToggleLabel(mode));
    btn?.setAttribute("title", themeToggleLabel(mode));
    if (icon) icon.textContent = themeToggleIcon(mode);
  });
}
