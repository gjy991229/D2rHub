import { useEffect, useState } from "react";
import { PET_ITEMS, PET_SLOTS } from "./catalog";
import { PetAvatar } from "./PetAvatar";
import { achievementLabel, achievementProgress, ownedCount } from "./progress";
import { connectWardrobe, settlePetActivity, usePetWardrobe } from "./wardrobeStore";

function ProgressBar({ label, value, max }: { label: string; value: number; max: number }) {
  return <div role="progressbar" aria-label={label} aria-valuenow={Math.min(value, max)} aria-valuemin={0} aria-valuemax={max}
    className="mt-2 h-1.5 overflow-hidden rounded-full bg-surface-hover">
    <div className="h-full rounded-full bg-accent" style={{ width: `${max > 0 ? Math.min(value / max, 1) * 100 : 0}%` }} />
  </div>;
}

export function PetProgressPanel({ english = false }: { english?: boolean }) {
  const { snapshot, error } = usePetWardrobe();
  const [filter, setFilter] = useState("all");
  const [busy, setBusy] = useState(false);
  useEffect(connectWardrobe, []);
  const refresh = async () => {
    setBusy(true);
    try { await settlePetActivity(); } catch { /* The store displays the save error. */ }
    finally { setBusy(false); }
  };
  const t = (zh: string, en: string) => english ? en : zh;
  const w = snapshot?.wardrobe;
  const achievements = PET_ITEMS.filter(item => item.source === "achievement");
  const unlocked = achievements.filter(item => w?.owned.includes(item.id)).length;
  const today = new Date();
  const day = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
  const number = (value: number) => value.toLocaleString(english ? "en-US" : "zh-CN");
  const button = "rounded-md border border-border-default px-3 py-1.5 text-xs hover:bg-surface-hover disabled:opacity-40";
  return <section className="space-y-4 text-text-primary" aria-label={t("成就与统计", "Achievements & stats")}>
    <div className="flex items-center justify-between gap-3"><div><h2 className="text-sm font-bold">{t("一起走过的日子", "Our journey together")}</h2><p className="text-xs text-text-muted mt-1">{t("进度约每分钟保存；手动刷新会结算当前活动。", "Progress saves about every minute. Refresh to settle current activity.")}</p></div><button className={button} disabled={busy} onClick={() => void refresh()}>{busy ? t("结算中…", "Saving…") : t("刷新统计", "Refresh stats")}</button></div>
    {error && <p role="alert" className="text-xs text-red-400">{error}</p>}
    {!w ? <p className="text-xs text-text-muted">{t("正在读取陪伴记录…", "Loading companionship records…")}</p> : <>
      <div className="grid grid-cols-2 lg:grid-cols-3 gap-3">{[
        [t("累计键鼠敲击", "Total inputs"), number(w.inputs ?? 0)],
        [t("有效陪伴", "Active companionship"), `${Math.floor(w.seconds / 3600)} ${t("小时", "h")} ${Math.floor(w.seconds % 3600 / 60)} ${t("分钟", "min")}`],
        [t("陪伴天数", "Companion days"), `${number(w.days)} ${t("天", "days")}`],
        [t("拥有饰品", "Accessories owned"), `${ownedCount(w)} / ${PET_ITEMS.length}`],
        [t("成就达成", "Achievements"), `${unlocked} / ${achievements.length}`],
        [t("可用碎片", "Available fragments"), number(w.fragments)],
      ].map(([label, value]) => <div key={label} className="spatial-panel p-4"><p className="text-xs text-text-muted">{label}</p><p className="mt-2 text-xl font-bold text-accent tabular-nums">{value}</p></div>)}</div>
      <p className="text-2xs text-text-muted">{t("累计敲击从新增统计版本开始记录，包含键盘按下和鼠标左右键；旧版本次数无法补算。小猫窗口的可重置计数独立于此累计值。仅保存次数，不保存按键内容。", "Lifetime inputs start with this version and include key presses and left/right mouse clicks; older counts cannot be recovered. The pet's resettable counter is separate. Only totals are stored, never key contents.")}</p>
      <div className="spatial-panel p-3 space-y-3"><h3 className="text-sm font-bold">{t("收藏进度", "Collection progress")}</h3><div className="grid grid-cols-2 sm:grid-cols-3 gap-3">{PET_SLOTS.map(slot => {
        const items = PET_ITEMS.filter(item => item.slot === slot.id);
        const count = items.filter(item => w.owned.includes(item.id)).length;
        return <div key={slot.id}><div className="flex justify-between text-xs"><span>{english ? slot.en : slot.name}</span><span>{count}/{items.length}</span></div><ProgressBar label={english ? slot.en : slot.name} max={items.length} value={count} /></div>;
      })}</div></div>
      <div className="flex flex-wrap items-center gap-2"><h3 className="text-sm font-bold mr-auto">{t("成就手册", "Achievement journal")}</h3>{[["all", t("全部", "All")], ["locked", t("进行中", "In progress")], ["unlocked", t("已达成", "Completed")]].map(([key, label]) => <button key={key} className={`${button} ${filter === key ? "text-accent border-accent" : ""}`} aria-pressed={filter === key} onClick={() => setFilter(key)}>{label}</button>)}</div>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">{achievements.filter(item => filter === "all" || (filter === "unlocked") === w.owned.includes(item.id)).map(item => {
        const done = w.owned.includes(item.id);
        return <article key={item.id} className={`spatial-panel p-3 space-y-2 ${done ? "border-accent" : ""}`}><div className="flex items-center gap-2"><PetAvatar equipped={{ [item.slot]: item.id }} width={86} /><div><h4 className="text-xs font-bold">{english ? item.en : item.name}</h4><p className="text-2xs text-accent mt-1">{done ? t("已达成 · 奖励已发放", "Completed · reward delivered") : t("进行中 · 达成自动发放", "In progress · automatic reward")}</p></div></div><p className="text-xs">{achievementLabel(item, w, english)}</p><ProgressBar label={english ? item.en : item.name} max={item.target ?? 0} value={achievementProgress(item, w)} /><p className="text-2xs text-text-muted">{t("奖励：专属饰品", "Reward: exclusive accessory")}{item.bonus_fragments ? ` + ${item.bonus_fragments} ${t("碎片", "fragments")}` : ""}</p></article>;
      })}</div>
      {filter === "unlocked" && unlocked === 0 && <p className="text-xs text-text-muted">{t("还没有达成成就，第一段旅途已经开始。", "No completed achievements yet. Your journey has begun.")}</p>}
      {filter === "locked" && unlocked === achievements.length && <p className="text-xs text-text-muted">{t("当前成就全部达成！", "All current achievements completed!")}</p>}
      <div className="spatial-panel p-4 space-y-2 text-xs"><h3 className="font-bold text-sm">{t("奖励规则", "Reward rules")}</h3><p>{t("今日随机机会", "Today's random rolls")}：{w.last_day === day ? w.daily_rolls : 0}/12 · {t("下次机会", "Next roll")}：{Math.floor(w.roll_seconds / 60)}/10 {t("分钟", "min")} · {t("距必得最多", "Guaranteed within")} {16 - w.misses} {t("次", "rolls")}</p><ul className="list-disc pl-4 space-y-2 text-text-muted">
        <li>{t("小猫可见且收到键鼠输入时计次；有输入的日期计为陪伴日，无需连续。有效时间仅累计相邻输入间不超过 30 秒的间隔，离开或隐藏不累计。", "Inputs count while the pet is visible. Dates with input count as companion days, with no streak required. Active time counts gaps of at most 30 seconds between inputs; absence and hidden time do not count.")}</li>
        <li>{t("每 10 分钟有效陪伴获得一次随机机会，10% 概率获得饰品；连续 15 次未中后，下次必得。每日最多 12 次，未用满的分钟可留到次日，达到上限后当天不再积攒抽取时间。", "Each 10 active minutes gives a roll with a 10% accessory chance. After 15 misses the next roll is guaranteed. Maximum 12 rolls per day; partial minutes carry over, but no roll time accrues after the daily cap.")}</li>
        <li>{t("未中获得 1 碎片，重复饰品获得 4 碎片；碎片可在衣柜定向兑换随机池饰品。成就只发放一次，不受每日随机机会限制。", "Misses grant 1 fragment, duplicates grant 4. Redeem fragments for random-pool accessories in the wardrobe. Achievement rewards are granted once and are not subject to daily roll limits.")}</li>
        <li>{t("收藏按不同饰品计数，初始、兑换和成就饰品都计入。成就饰品及附赠碎片自动发放；现有陪伴和收藏进度可解锁新成就。", "Collection counts unique starter, redeemed and achievement accessories. Exclusive items and bonus fragments arrive automatically. Existing companionship and collection progress qualifies for new achievements.")}</li>
      </ul></div>
    </>}
  </section>;
}
