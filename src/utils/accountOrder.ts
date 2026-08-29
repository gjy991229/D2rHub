import type { AccountMeta } from "../store/types";

type OrderedAccount = Pick<AccountMeta, "order">;

export function sortAccountsByCardOrder<T extends OrderedAccount>(accounts: readonly T[]): T[] {
  return [...accounts].sort((left, right) => left.order - right.order);
}
