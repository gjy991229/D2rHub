/** A stable tint follows the account; the letters follow its display name. */
export function AccountMonogram({ id, name }: { id: string; name: string }) {
  const words = name.trim().split(/[\s_-]+/u).filter(Boolean);
  const initials = words.length > 1
    ? [Array.from(words[0])[0], Array.from(words[words.length - 1])[0]].join("")
    : Array.from(words[0] || id).slice(0, 2).join("");
  let tone = 0;
  for (const char of id) tone = (tone * 31 + char.charCodeAt(0)) >>> 0;
  return <span aria-hidden="true" data-i18n-skip data-tone={tone % 4} className="account-monogram">{initials.toLocaleUpperCase()}</span>;
}
