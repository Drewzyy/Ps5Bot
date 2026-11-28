// Plain messages for update problems. The updater plugin's own error texts are short and technical
// ("Could not fetch a valid release JSON from the remote"); these turn them into something the person can act on.
// Tested in updaterText.test.ts.

export const NOT_SET_UP = "Auto-update isn't set up yet. Add the signing key on GitHub (see Settings > About)";

/** The steps shown under that message and in Settings > About. */
export const NOT_SET_UP_STEPS = [
  "Open your repository on GitHub, then Settings, Secrets and variables, Actions.",
  "Press New repository secret. Name it TAURI_SIGNING_PRIVATE_KEY.",
  "Paste the whole text of your private key file (the one made by npx tauri signer generate, made without a password). Press Add secret.",
  "Open the Actions tab, pick windows-build and press Run workflow (or push any change). When it finishes, a release appears and the update button works.",
  "No key yet? Run npx tauri signer generate -w claude-trader.key, add its private text as the secret, and ask Claude to put the new public key in the app.",
];

export type UpdateErrorKind = "not-set-up" | "offline" | "signature" | "bad-release" | "other";

export function classifyUpdateError(raw: string): UpdateErrorKind {
  const r = String(raw ?? "");
  if (/error sending request|dns error|failed to lookup|connection (refused|reset|closed)|timed out|timeout|network|offline|unreachable|os error/i.test(r)) return "offline";
  if (/valid release json|release.?not.?found|\b404\b|\b403\b|not found|no release/i.test(r)) return "not-set-up";
  if (/signature|minisign|pubkey|public key/i.test(r)) return "signature";
  if (/deserial|missing field|invalid (type|value|length)|expected (value|a )|unexpected (end|token)|json/i.test(r)) return "bad-release";
  return "other";
}

export function updateErrorMessage(raw: string): { kind: UpdateErrorKind; message: string } {
  const kind = classifyUpdateError(raw);
  switch (kind) {
    case "not-set-up":
      return { kind, message: NOT_SET_UP };
    case "offline":
      return { kind, message: "Could not reach GitHub. Check your internet connection and try again." };
    case "signature":
      return { kind, message: "An update was found, but its signature does not match this app. The signing key on GitHub may not be the one this app was built for. See Settings > About." };
    case "bad-release":
      return { kind, message: "The update file on GitHub could not be read. The next build usually fixes this." };
    default:
      return { kind, message: `Could not check for updates: ${String(raw ?? "").slice(0, 200)}` };
  }
}
