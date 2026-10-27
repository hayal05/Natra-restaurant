// Do not present logout as completed while native authority remains active.
export async function endNativeSession({logout, clearUser, navigate}) {
  await logout();
  clearUser();
  navigate("/login");
}
