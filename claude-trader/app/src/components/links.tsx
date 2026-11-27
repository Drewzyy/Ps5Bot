import type { MouseEvent } from "react";
import { api } from "../source";

/** Short plain label for a source link: "X post" (one tweet), "X profile" (an account page), "Telegram" or "website". */
export function linkLabel(url: string): string {
  let host = "";
  let path = "";
  try {
    const u = new URL(url);
    host = u.hostname.replace(/^www\./, "");
    path = u.pathname;
  } catch {
    return "link";
  }
  if ((host === "x.com" || host === "twitter.com") && /\/status\/\d+/.test(path)) return "X post";
  if ((host === "x.com" || host === "twitter.com") && /^\/i\/communities\//.test(path)) return "X community";
  if (host === "x.com" || host === "twitter.com") return "X profile";
  if (host === "t.me" || host === "telegram.me") return "Telegram";
  return "website";
}

/** "Source: website" style link that opens in the browser. Renders nothing without a url. */
export function SourceLink({ url, prefix = "Source" }: { url?: string | null; prefix?: string }) {
  if (!url) return null;
  const go = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    api.openLink(url);
  };
  return (
    <a className="srclink" href={url} onClick={go} rel="noreferrer" title={url}>
      {prefix ? `${prefix}: ` : ""}{linkLabel(url)}
    </a>
  );
}
