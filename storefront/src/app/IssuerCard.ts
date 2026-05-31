import { api, type Issuer } from "../services/api";

export function renderIssuerCard(root: HTMLElement): void {
  root.innerHTML = `
    <section class="card">
      <h3>Register an issuer</h3>
      <p class="card-sub">Creates a tenant record. The portal mints a UUID that doubles
        as the on-chain <code>issuer_id</code> (32-char hex) you pass to the kyc-hook CLI.</p>
      <form id="issuer-form" class="form-grid">
        <label>Issuer name <input name="name" required placeholder="AetherVane RWA Inc." /></label>
        <label>Ops authority (base58, optional)
          <input name="ops_authority" placeholder="SELdziqd…" /></label>
        <label>Identity / cold root (base58, optional)
          <input name="identity" placeholder="optional" /></label>
        <label>Contact email (optional)
          <input name="contact_email" type="email" placeholder="ops@issuer.example" /></label>
        <button type="submit" class="btn btn-primary">Register issuer</button>
      </form>
      <p class="form-msg" id="issuer-msg"></p>
      <div id="issuer-result"></div>

      <hr class="card-rule" />
      <h3>Look up an issuer</h3>
      <form id="lookup-form" class="form-inline">
        <input name="issuer_id" placeholder="UUID or 32-char hex issuer_id" required />
        <button type="submit" class="btn btn-ghost">Look up</button>
      </form>
      <div id="lookup-result"></div>
    </section>
  `;

  const issuerForm = root.querySelector("#issuer-form") as HTMLFormElement;
  const issuerMsg = root.querySelector("#issuer-msg") as HTMLElement;
  const issuerResult = root.querySelector("#issuer-result") as HTMLElement;

  issuerForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    issuerMsg.textContent = "Registering…";
    issuerMsg.className = "form-msg";
    const fd = new FormData(issuerForm);
    try {
      const issuer = await api.registerIssuer({
        name: String(fd.get("name") || "").trim(),
        ops_authority: strOrUndef(fd.get("ops_authority")),
        identity: strOrUndef(fd.get("identity")),
        contact_email: strOrUndef(fd.get("contact_email")),
      });
      issuerMsg.textContent = "Issuer registered.";
      issuerMsg.className = "form-msg is-ok";
      issuerResult.innerHTML = issuerCardHtml(issuer);
      issuerForm.reset();
    } catch (err) {
      issuerMsg.textContent = errMsg(err);
      issuerMsg.className = "form-msg is-err";
    }
  });

  const lookupForm = root.querySelector("#lookup-form") as HTMLFormElement;
  const lookupResult = root.querySelector("#lookup-result") as HTMLElement;
  lookupForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    const fd = new FormData(lookupForm);
    const id = String(fd.get("issuer_id") || "").trim();
    lookupResult.innerHTML = `<p class="form-msg">Looking up…</p>`;
    try {
      const issuer = await api.getIssuer(id);
      lookupResult.innerHTML = issuerCardHtml(issuer);
    } catch (err) {
      lookupResult.innerHTML = `<p class="form-msg is-err">${errMsg(err)}</p>`;
    }
  });
}

function issuerCardHtml(i: Issuer): string {
  return `
    <div class="result-box">
      <div class="kv"><span>Name</span><strong>${esc(i.name)}</strong></div>
      <div class="kv"><span>Status</span><span class="status-pill status-${esc(i.status)}">${esc(i.status)}</span></div>
      <div class="kv"><span>UUID</span><code class="copyable">${esc(i.id)}</code></div>
      <div class="kv"><span>On-chain issuer_id</span><code class="copyable">${esc(i.issuer_id_hex)}</code></div>
      ${i.ops_authority ? `<div class="kv"><span>Ops authority</span><code>${esc(i.ops_authority)}</code></div>` : ""}
    </div>
  `;
}

function strOrUndef(v: FormDataEntryValue | null): string | undefined {
  const s = String(v ?? "").trim();
  return s === "" ? undefined : s;
}

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

export function errMsg(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
