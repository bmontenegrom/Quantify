let csrfToken = null;

/** Guarda el token CSRF recibido del servidor (en login y en /auth/me). */
export function setCsrfToken(token) {
  csrfToken = token ?? null;
}

export async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(await errorText(response));
  return response.json();
}

export async function postJson(url, payload) {
  const headers = { "content-type": "application/json" };
  if (csrfToken) headers["x-csrf-token"] = csrfToken;
  const response = await fetch(url, {
    method: "POST",
    headers,
    body: JSON.stringify(payload),
  });
  if (!response.ok) throw new Error(await errorText(response));
  return response.json();
}

export async function deleteJson(url) {
  const headers = {};
  if (csrfToken) headers["x-csrf-token"] = csrfToken;
  const response = await fetch(url, { method: "DELETE", headers });
  if (!response.ok) throw new Error(await errorText(response));
  return response.json();
}

export async function errorText(response) {
  try {
    const body = await response.json();
    return body.error ?? response.statusText;
  } catch {
    return response.statusText;
  }
}
