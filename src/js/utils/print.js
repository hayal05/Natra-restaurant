import {escapeHtml} from "./html.js";
const policy="default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src 'self' data:; base-uri 'none'; form-action 'none'";
// This frame cannot execute scripts, open popups or navigate its parent.
export function createPrintFrame(html,title="Print document") {
  const frame=document.createElement("iframe");
  frame.setAttribute("sandbox","allow-same-origin allow-modals");
  frame.title=title;frame.style.cssText="position:fixed;width:0;height:0;border:0;opacity:0";
  const meta=`<meta http-equiv="Content-Security-Policy" content="${escapeHtml(policy)}">`;
  const source=String(html);
  frame.srcdoc=/<head\b[^>]*>/i.test(source)?source.replace(/<head\b[^>]*>/i,head=>head+meta):`<!doctype html><html><head>${meta}</head><body>${source}</body></html>`;
  return frame;
}
export function printHtml(html,title) {
  const frame=createPrintFrame(html,title);
  frame.addEventListener("load",()=>{
    const win=frame.contentWindow;
    win.addEventListener("afterprint",()=>frame.remove(),{once:true});
    try{win.focus();win.print();}catch{frame.remove();}
  },{once:true});
  document.body.append(frame);
  return frame;
}
export function tablePrintHtml(title,summary,headings,rows) {
  return `<!doctype html><html><head><meta charset="utf-8"><title>${escapeHtml(title)}</title>
    <style>body{font:12px Arial,sans-serif;color:#111}table{border-collapse:collapse;width:100%}th,td{border:1px solid #ccc;padding:6px;text-align:left;overflow-wrap:anywhere}</style>
    </head><body><h2>${escapeHtml(title)}</h2><p>${escapeHtml(summary)}</p><table><thead><tr>${headings.map(v=>`<th>${escapeHtml(v)}</th>`).join("")}</tr></thead>
    <tbody>${rows.map(row=>`<tr>${row.map(v=>`<td>${escapeHtml(v)}</td>`).join("")}</tr>`).join("")}</tbody></table></body></html>`;
}
