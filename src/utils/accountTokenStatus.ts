export const BATTLE_NET_TOKEN_VALID_HOURS = 720;

export interface AccountTokenStatus {
  expired: boolean;
  warning: boolean;
  remainingHours: number;
  label: string;
}

export function getAccountTokenStatus(
  lastResetAt: string | null | undefined,
  nowMs = Date.now(),
): AccountTokenStatus {
  if (!lastResetAt) {
    return { expired: false, warning: false, remainingHours: 0, label: "" };
  }

  const resetMs = new Date(lastResetAt).getTime();
  if (Number.isNaN(resetMs)) {
    return { expired: false, warning: false, remainingHours: 0, label: "" };
  }

  const elapsedHours = (nowMs - resetMs) / (1000 * 60 * 60);
  const remaining = Math.max(0, BATTLE_NET_TOKEN_VALID_HOURS - elapsedHours);

  if (remaining <= 0) {
    return { expired: true, warning: true, remainingHours: 0, label: "过期" };
  }
  if (remaining <= 48) {
    return {
      expired: false,
      warning: true,
      remainingHours: remaining,
      label: `${Math.floor(remaining)}h`,
    };
  }

  return {
    expired: false,
    warning: false,
    remainingHours: remaining,
    label: `${Math.floor(remaining / 24)}d`,
  };
}
