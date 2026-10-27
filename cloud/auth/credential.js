// A provisioned random secret, never a local password or hardware/installation ID.
export const credentialPattern=/^natra_sync_[A-Za-z0-9_-]{43}$/;
export function newSyncCredential() {
  const bytes=crypto.getRandomValues(new Uint8Array(32));
  return "natra_sync_"+btoa(String.fromCharCode(...bytes)).replace(/\+/g,"-").replace(/\//g,"_").replace(/=+$/,"");
}
export async function credentialDigest(secret) {
  if(typeof secret!=="string" || secret.length!==54 || !credentialPattern.test(secret))throw new Error("Invalid sync credential");
  return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",new TextEncoder().encode(secret))),
    byte=>byte.toString(16).padStart(2,"0")).join("");
}
export function localUserId(value) {
  if(typeof value!=="string" || value.trim()!==value || !/^[1-9][0-9]{0,18}$/.test(value) || BigInt(value)>9223372036854775807n)
    throw new Error("Invalid native user identity");
  return value;
}
