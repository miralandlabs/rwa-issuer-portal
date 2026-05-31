import { api } from "../services/api";
import { errMsg } from "./IssuerCard";

export function renderKycCard(root: HTMLElement): void {
  root.innerHTML = `
    <section class="card">
      <h3>Submit investor KYC</h3>
      <p class="card-sub">Records a KYC request for an investor wallet under an issuer.
        Portal admin reviews it; once approved, the ops sync writes the on-chain KycRecord.</p>
      <form id="kyc-form" class="form-grid">
        <label>Issuer id (UUID or hex)
          <input name="issuer_id" required placeholder="550e8400-e29b-41d4-a716-446655440000" /></label>
        <label>Investor wallet (base58)
          <input name="wallet" required placeholder="buyA5hR1Z9Kt…" /></label>
        <label>Scope
          <select name="scope" id="kyc-scope">
            <option value="global">global</option>
            <option value="offering">offering</option>
          </select>
        </label>
        <label id="offering-wrap" hidden>Offering id (≤ 31 chars)
          <input name="offering_id" placeholder="series-a" maxlength="31" /></label>
        <button type="submit" class="btn btn-primary">Submit KYC</button>
      </form>
      <p class="form-msg" id="kyc-msg"></p>
      <div id="kyc-result"></div>
    </section>
  `;

  const form = root.querySelector("#kyc-form") as HTMLFormElement;
  const scope = root.querySelector("#kyc-scope") as HTMLSelectElement;
  const offeringWrap = root.querySelector("#offering-wrap") as HTMLElement;
  const msg = root.querySelector("#kyc-msg") as HTMLElement;
  const result = root.querySelector("#kyc-result") as HTMLElement;

  const syncOffering = () => {
    offeringWrap.hidden = scope.value !== "offering";
  };
  scope.addEventListener("change", syncOffering);
  syncOffering();

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    msg.textContent = "Submitting…";
    msg.className = "form-msg";
    const fd = new FormData(form);
    const scopeVal = String(fd.get("scope"));
    const offering = String(fd.get("offering_id") || "").trim();
    try {
      const rec = await api.submitKyc({
        issuer_id: String(fd.get("issuer_id") || "").trim(),
        wallet: String(fd.get("wallet") || "").trim(),
        scope: scopeVal,
        offering_id: scopeVal === "offering" && offering ? offering : undefined,
      });
      msg.textContent = `KYC submitted (record #${rec.id}, status: ${rec.status}).`;
      msg.className = "form-msg is-ok";
      result.innerHTML = `
        <div class="result-box">
          <div class="kv"><span>Record</span><strong>#${rec.id}</strong></div>
          <div class="kv"><span>Status</span><span class="status-pill status-${rec.status}">${rec.status}</span></div>
          <div class="kv"><span>Scope</span><span>${rec.scope}${rec.offering_id ? " · " + rec.offering_id : ""}</span></div>
          <div class="kv"><span>Verified</span><span>${rec.is_verified ? "yes" : "no"}</span></div>
        </div>`;
    } catch (err) {
      msg.textContent = errMsg(err);
      msg.className = "form-msg is-err";
    }
  });
}
