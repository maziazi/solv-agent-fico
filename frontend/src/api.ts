// The one place the desk talks to the Rust service. Same-origin calls only;
// `pnpm dev` reaches the server through the Vite proxy (vite.config.ts).

import type {
  ReceiptResponse,
  DirectoryResponse,
  PreSpendCheckRequest,
  PreSpendCheckResponse,
} from "./types";

export class ApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

async function parse<T>(response: Response): Promise<T> {
  if (!response.ok) {
    let detail = `HTTP ${response.status}`;
    try {
      const body = (await response.json()) as { error?: string };
      if (typeof body.error === "string") detail = body.error;
    } catch {
      // A non-JSON error body; the status line is the honest fallback.
    }
    throw new ApiError(response.status, detail);
  }
  return (await response.json()) as T;
}

export async function preSpendCheck(
  request: PreSpendCheckRequest,
): Promise<PreSpendCheckResponse> {
  const response = await fetch("/api/v1/pre-spend-check", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
  });
  return parse<PreSpendCheckResponse>(response);
}

export async function fetchDirectory(): Promise<DirectoryResponse> {
  const response = await fetch("/api/v1/directory");
  return parse<DirectoryResponse>(response);
}

export async function fetchReceipt(id: string): Promise<ReceiptResponse> {
  const response = await fetch(`/api/v1/receipt/${encodeURIComponent(id)}`);
  return parse<ReceiptResponse>(response);
}
