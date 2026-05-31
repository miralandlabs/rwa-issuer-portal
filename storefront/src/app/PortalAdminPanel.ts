import { api, type SyncFeedItem } from "../services/api";
import { errMsg } from "./IssuerCard";

// Portal admin bearer token — in-memory only (never persisted).
let bearer = "";

export function renderPortalAdminPanel(root: HTMLElement): void {
  root.innerHTML = `
    <section class="card">
      <h3>Portal admin</h3>
      <p class="card-sub">One admin per deployment. Review KYC, set issuer status,
        and work the on-chain sync feed. Token is held in memory only.</p>
      <label>Admin token
        <input id="admin-token" type="password" placeholder="bearer token" autocomplete="off" /></label>

      <div class="admin-actions">
        <form id="review-form" class="form-inline">
          <input name="id" type="number" min="1" placeholder="kyc id" required />
          <select name="decision">
            <option value="approved">approve</option>
            <option value="rejected">reject</option>
          </select>
          <input name="review_note" placeholder="note (optional)" />
          <button type="submit" class="btn btn-primary">Review</button>
        </form>
      </div>
      <p class="form-msg" id="admin-msg"></p>

      <hr class="card-rule" />
      <div class="feed-head">
        <h3>On-chain sync feed</h3>
        <button type="button" class="btn btn-ghost" id="feed-refresh">Refresh</button>
      </div>
      <p class="card-sub">Decisions whose on-chain KycRecord is stale. Run the kyc-hook
        ops CLI for each row, then mark synced.</p>
      <div id="feed"></div>
    </section>
  `;

  const tokenInput = root.querySelector("#admin-token") as HTMLInputElement;
  tokenInput.addEventListener("input", () => (bearer = tokenInput.value.trim()));

  const msg = root.querySelector("#admin-msg") as HTMLElement;
  const reviewForm = root.querySelector("#review-form") as HTMLFormElement;
  reviewForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!bearer) return setMsg(msg, "Enter the admin token first.", true);
    const fd = new FormData(reviewForm);
    try {
      const rec = await api.reviewKyc(bearer, {
        id: Number(fd.get("id")),
        decision: String(fd.get("decision")) as "approved" | "rejected",
        review_note: strOrUndef(fd.get("review_note")),
      });
      setMsg(msg, `Record #${rec.id} → ${rec.status} (verified: ${rec.is_verified}).`, false);
      void loadFeed();
    } catch (err) {
      setMsg(msg, errMsg(err), true);
    }
  });

  const feed = root.querySelector("#feed") as HTMLElement;
  const refresh = root.querySelector("#feed-refresh") as HTMLButtonElement;
  refresh.addEventListener("click", () => void loadFeed());

  async function loadFeed(): Promise<void> {
    if (!bearer) {
      feed.innerHTML = `<p class="form-msg">Enter the admin token to load the feed.</p>`;
      return;
    }
    feed.innerHTML = `<p class="form-msg">Loading…</p>`;
    try {
      const { items, count } = await api.syncFeed(bearer);
      feed.innerHTML = count === 0 ? `<p class="form-msg">Nothing pending — all synced.</p>` : feedTable(items);
      feed.querySelectorAll<HTMLButtonElement>("button[data-mark]").forEach((b) => {
        b.addEventListener("click", async () => {
          try {
            await api.markSynced(bearer, Number(b.dataset.mark));
            void loadFeed();
          } catch (err) {
            setMsg(msg, errMsg(err), true);
          }
        });
      });
    } catch (err) {
      feed.innerHTML = `<p class="form-msg is-err">${errMsg(err)}</p>`;
    }
  }
}

function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function feedTable(items: SyncFeedItem[]): string {
  const rows = items
    .map(
      (it) => `
      <tr>
        <td>${it.id}</td>
        <td><code>${esc(it.issuer_id_hex)}</code></td>
        <td><code>${esc(it.wallet)}</code></td>
        <td>${esc(it.scope)}${it.offering_id ? " · " + esc(it.offering_id) : ""}</td>
        <td>${it.is_verified ? "verify" : "revoke"}</td>
        <td><button class="btn btn-ghost btn-sm" data-mark="${it.id}">Mark synced</button></td>
      </tr>`,
    )
    .join("");
  return `
    <div class="table-wrap">
      <table class="feed-table">
        <thead><tr><th>#</th><th>issuer_id</th><th>wallet</th><th>scope</th><th>action</th><th></th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;
}

function setMsg(el: HTMLElement, text: string, isErr: boolean): void {
  el.textContent = text;
  el.className = `form-msg ${isErr ? "is-err" : "is-ok"}`;
}

function strOrUndef(v: FormDataEntryValue | null): string | undefined {
  const s = String(v ?? "").trim();
  return s === "" ? undefined : s;
}
