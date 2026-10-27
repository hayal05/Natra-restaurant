/** Text/quoted-attribute escaping for the few trusted templates that need HTML. */
export const escapeHtml=value=>String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));

// Uploaded portraits are re-encoded as JPEG; imported raster data is supported.
// Remote/file/SVG URLs are deliberately not a display or tracking channel.
export function safePhotoUrl(value) {
  if(typeof value!=="string" || value.length>2*1024*1024)return null;
  const match=/^data:image\/(?:jpeg|png|gif|webp);base64,([A-Za-z0-9+/]+={0,2})$/.exec(value);
  return match && match[0].length===value.length && match[1].length%4===0 ? value : null;
}
export function setPortrait(container,url,fallback="?") {
  container.replaceChildren();
  const safe=safePhotoUrl(url);
  if(safe){const img=document.createElement("img");img.src=safe;img.alt="";container.append(img);}
  else container.textContent=String(fallback||"?").trim().charAt(0).toUpperCase()||"?";
}
