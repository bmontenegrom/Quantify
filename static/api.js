let csrfToken = null;

export function setCsrfToken(token) {
  csrfToken = token ?? null;
}

async function send(url, options) {
  try {
    return await fetch(url, options);
  } catch {
    throw new Error("No se pudo conectar con el servidor. Revisá tu conexión y reintentá.");
  }
}

export async function fetchJson(url) {
  const response = await send(url);
  if (!response.ok) throw new Error(await errorText(response));
  return response.json();
}

export async function postJson(url, payload) {
  const headers = { "content-type": "application/json" };
  if (csrfToken) headers["x-csrf-token"] = csrfToken;
  const response = await send(url, {
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
  const response = await send(url, { method: "DELETE", headers });
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
