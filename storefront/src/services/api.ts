// Thin client for the rwa-issuer-portal backend. Admin endpoints require the
// portal-admin bearer token (in-memory in the UI, never persisted).

export interface Issuer {
  id: string;
  issuer_id_hex: string;
  name: string;
  status: string;
  ops_authority: string | null;
  identity: string | null;
  contact_email: string | null;
  created_at: string;
}

export interface KycRecord {
  id: number;
  issuer_id: string;
  issuer_id_hex: string;
  wallet: string;
  scope: string;
  offering_id: string | null;
  status: string;
  is_verified: boolean;
  synced_on_chain: boolean;
  review_note: string | null;
}

export interface SyncFeedItem {
  id: number;
  issuer_id_hex: string;
  wallet: string;
  scope: string;
  offering_id: string | null;
  is_verified: boolean;
}

async function call<T>(
  method: string,
  path: string,
  body?: unknown,
  bearer?: string,
): Promise<T> {
  const headers: Record<string, string> = {};
  if (body !== undefined) headers["Content-Type"] = "application/json";
  if (bearer) headers["Authorization"] = `Bearer ${bearer}`;
  const res = await fetch(path, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  const json = text ? JSON.parse(text) : {};
  if (!res.ok) {
    throw new Error(json.message || `${res.status} ${res.statusText}`);
  }
  return json as T;
}

export const api = {
  registerIssuer(input: {
    name: string;
    ops_authority?: string;
    identity?: string;
    contact_email?: string;
  }): Promise<Issuer> {
    return call("POST", "/api/v1/issuers", input);
  },

  getIssuer(issuerId: string): Promise<Issuer> {
    return call("GET", `/api/v1/issuers?issuer_id=${encodeURIComponent(issuerId)}`);
  },

  submitKyc(input: {
    issuer_id: string;
    wallet: string;
    scope: string;
    offering_id?: string;
  }): Promise<KycRecord> {
    return call("POST", "/api/v1/kyc", input);
  },

  // ---- Portal admin ----
  reviewKyc(
    bearer: string,
    input: { id: number; decision: "approved" | "rejected"; review_note?: string },
  ): Promise<KycRecord> {
    return call("POST", "/api/v1/kyc/review", input, bearer);
  },

  setIssuerStatus(
    bearer: string,
    input: { issuer_id: string; status: string },
  ): Promise<Issuer> {
    return call("PATCH", "/api/v1/issuers", input, bearer);
  },

  syncFeed(bearer: string, limit = 100): Promise<{ items: SyncFeedItem[]; count: number }> {
    return call("GET", `/api/v1/sync/feed?limit=${limit}`, undefined, bearer);
  },

  markSynced(bearer: string, id: number): Promise<KycRecord> {
    return call("POST", "/api/v1/sync/mark", { id }, bearer);
  },

  listKyc(
    bearer: string,
    params?: { status?: string; issuer_id?: string; limit?: number },
  ): Promise<{ items: KycRecord[]; count: number }> {
    const qs = new URLSearchParams();
    if (params?.status) qs.set("status", params.status);
    if (params?.issuer_id) qs.set("issuer_id", params.issuer_id);
    if (params?.limit) qs.set("limit", String(params.limit));
    const q = qs.toString();
    return call("GET", `/api/v1/kyc${q ? `?${q}` : ""}`, undefined, bearer);
  },
};
