// UI affordances only. Native commands independently enforce this policy.
export const landingFor = user => user?.role === "admin" ? "/dashboard" : "/pos";
export const canVisit = (user, path) => !!user && (user.role === "admin" || (user.role === "staff" && path === "/pos"));
