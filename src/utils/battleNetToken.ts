const TOKEN_PATTERN = /(?:^|[^A-Za-z0-9_-])((?:CN|US|KR|EU)-[A-Za-z0-9]{16,128}-[A-Za-z0-9]{6,64})(?=$|[^A-Za-z0-9_-])/i;
const ST_ASSIGNMENT_PATTERN = /(?:^|[^A-Za-z0-9_])ST\s*=\s*/gi;
const URL_VALUE_END_PATTERN = /[&＆#\r\n]/;

function findCompleteToken(value: string): string | null {
  return value.match(TOKEN_PATTERN)?.[1] ?? null;
}

/**
 * Extracts the Battle.net session token from pasted browser text.
 *
 * The preferred input is the complete redirect URL: the ST value is isolated
 * before scanning it, so unrelated URL parameters and surrounding Chinese copy
 * cannot become part of the credential. A bare token remains supported for
 * users following an older version of the guide.
 */
export function extractBattleNetToken(input: string): string | null {
  const pasted = input.trim();
  if (!pasted) return null;

  ST_ASSIGNMENT_PATTERN.lastIndex = 0;
  for (let assignment = ST_ASSIGNMENT_PATTERN.exec(pasted); assignment; assignment = ST_ASSIGNMENT_PATTERN.exec(pasted)) {
    const valueStart = assignment.index + assignment[0].length;
    const remainder = pasted.slice(valueStart);
    const valueEnd = remainder.search(URL_VALUE_END_PATTERN);
    const parameterValue = valueEnd >= 0 ? remainder.slice(0, valueEnd) : remainder;
    const token = findCompleteToken(parameterValue);
    if (token) return token;
  }

  return findCompleteToken(pasted);
}
