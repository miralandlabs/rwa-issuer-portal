import { renderHeader } from "./Header";
import { renderIssuerCard } from "./IssuerCard";
import { renderKycCard } from "./KycCard";
import { renderOperatorPanel } from "./OperatorPanel";

export function bootstrap(root: HTMLElement): void {
  const shell = document.createElement("div");
  shell.className = "shell";

  const headerMount = document.createElement("div");
  renderHeader(headerMount);

  const hero = document.createElement("section");
  hero.className = "hero";
  hero.innerHTML = `
    <h2>RWA issuer registration & investor KYC</h2>
    <p>The off-chain system of record for the x402 <code>rwa-kyc-hook</code>. Register an
       issuer, collect investor KYC, and feed approved decisions to the on-chain sync — the
       Transfer Hook enforces eligibility at settlement time.</p>
  `;

  const grid = document.createElement("div");
  grid.className = "card-grid";
  const issuerMount = document.createElement("div");
  const kycMount = document.createElement("div");
  renderIssuerCard(issuerMount);
  renderKycCard(kycMount);
  grid.append(issuerMount, kycMount);

  const opMount = document.createElement("div");
  renderOperatorPanel(opMount);

  shell.append(headerMount, hero, grid, opMount);
  root.replaceChildren(shell);
}
