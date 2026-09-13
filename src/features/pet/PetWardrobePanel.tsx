import { useEffect, useState } from "react";
import { PET_ITEMS, PET_SLOTS } from "./catalog";
import { PetAvatar } from "./PetAvatar";
import { changeWardrobe, connectWardrobe, reloadWardrobe, usePetWardrobe } from "./wardrobeStore";
import type { PetAction, PetFrame, PetSlot, PetTone } from "./types";

const button = "rounded-md border border-border-default px-2 py-1 text-xs hover:bg-surface-hover disabled:opacity-40 disabled:cursor-not-allowed";
export function PetWardrobePanel({ english = false }: { english?: boolean }) {
  const { snapshot, error } = usePetWardrobe();
  const [slot, setSlot] = useState<PetSlot>("head");
  const [frame, setFrame] = useState<PetFrame>("up");
  const [preview, setPreview] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(connectWardrobe, []);
  const act = async (action: PetAction) => {
    setBusy(true);
    try { await changeWardrobe(action); setPreview(null); } catch { /* Store exposes the error and retains confirmed state. */ }
    finally { setBusy(false); }
  };
  const w = snapshot?.wardrobe;
  const today = new Date();
  const dayKey = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
  const text = (zh: string, en: string) => english ? en : zh;
  return <section className="spatial-panel p-3 space-y-3 text-text-primary" aria-label={text("装扮衣柜", "Wardrobe")}>
    <div className="flex items-center justify-between"><h3 className="text-sm font-bold">{text("装扮衣柜", "Wardrobe")}</h3>
      {w && <span className="text-xs text-accent">{text("碎片", "Fragments")} {w.fragments} · {w.owned.length}/{PET_ITEMS.length}</span>}</div>
    {error && <div role="alert" className="text-xs text-red-400">{error} <button className={button} onClick={() => void reloadWardrobe()}>{text("重试", "Retry")}</button></div>}
    {!w ? <p className="text-xs text-text-muted">{text("正在读取衣柜…", "Loading wardrobe…")}</p> : <>
      <div className="flex flex-wrap items-center justify-center gap-3 rounded-lg bg-surface-hover p-2">
        <PetAvatar frame={frame} equipped={preview ? { ...w.equipped, [slot]: preview } : w.equipped} />
        <div className="space-y-2 text-xs">
          <p>{preview ? text("试穿预览 · 尚未装备", "Preview · not equipped") : text("当前搭配", "Current outfit")}</p>
          <div className="flex gap-1">{(["up", "left", "right"] as PetFrame[]).map((value, index) => <button key={value} aria-pressed={frame === value} className={button} onClick={() => setFrame(value)}>{(english ? ["Idle", "Left", "Right"] : ["抬手", "左手", "右手"])[index]}</button>)}</div>
          <div className="flex gap-1"><button className={button} disabled={busy} onClick={() => void act({ kind: "clear" })}>{text("全部卸下", "Clear outfit")}</button>{preview && <button className={button} onClick={() => setPreview(null)}>{text("结束试穿", "End preview")}</button>}</div>
        </div>
      </div>
      <div className="flex flex-wrap gap-1" aria-label={text("装扮部位", "Slots")}>{PET_SLOTS.map(s => <button key={s.id} className={`${button} ${slot === s.id ? "text-accent border-accent" : ""}`} aria-pressed={slot === s.id} onClick={() => { setSlot(s.id); setPreview(null); }}>{english ? s.en : s.name}</button>)}</div>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">{PET_ITEMS.filter(item => item.slot === slot).map(item => {
        const owned = w.owned.includes(item.id);
        const equipped = w.equipped[slot] === item.id;
        const current = item.metric === "days" ? w.days : Math.floor(w.seconds / 3600);
        const target = item.metric === "days" ? item.target! : item.target! / 3600;
        const weightTotal = PET_ITEMS.reduce((sum, entry) => sum + entry.weight, 0);
        return <article key={item.id} className={`rounded-lg border p-2 space-y-2 ${equipped ? "border-accent" : "border-border-default"}`}>
          <button className="flex w-full items-center gap-2 text-left" onClick={() => setPreview(item.id)} aria-label={`${text("试穿", "Preview")} ${english ? item.en : item.name}`}>
            <PetAvatar equipped={{ [slot]: item.id }} width={75} /><span className="text-xs font-semibold">{english ? item.en : item.name}<span className="block font-normal text-text-muted">{equipped ? text("已装备", "Equipped") : owned ? text("已拥有", "Owned") : text("未解锁 · 可试穿", "Locked · preview available")}</span></span>
          </button>
          <p className="text-2xs text-text-muted">{item.source === "starter" ? text("初始赠送", "Starter gift") : item.source === "achievement" ? `${text("成就：累计陪伴", "Achievement: companionship")} ${Math.min(current, target)}/${target} ${item.metric === "days" ? text("天（无需连续）", "days, non-consecutive") : text("小时", "hours")}` : `${text("随机装扮池占比", "Share of accessory pool")} ${(item.weight / weightTotal * 100).toFixed(1)}% · ${item.cost} ${text("碎片可定向兑换", "fragments to redeem")}`}</p>
          <div className="flex gap-1">{owned ? <button disabled={busy} className={button} onClick={() => void act(equipped ? { kind: "unequip", slot } : { kind: "equip", id: item.id })}>{equipped ? text("卸下", "Remove") : text("装备", "Equip")}</button> : item.source === "random" ? <button className={button} disabled={busy || w.fragments < item.cost} onClick={() => void act({ kind: "redeem", id: item.id })}>{text("兑换", "Redeem")} · {item.cost}</button> : null}</div>
        </article>;
      })}</div>
      <div className="space-y-2 border-t border-border-default pt-3">
        <p className="text-xs">{text("陪伴", "Companionship")} {(w.seconds / 3600).toFixed(1)} {text("小时", "hours")} · {w.days} {text("天", "days")}</p>
        <p className="text-xs">{text("下次机会", "Next opportunity")} {Math.floor(w.roll_seconds / 60)}/10 {text("分钟", "min")} · {text("今日机会", "Today's rolls")} {w.last_day === dayKey ? w.daily_rolls : 0}/12 · {text("距必得最多", "Guaranteed within")} {16 - w.misses} {text("次", "rolls")}</p>
        <p className="text-2xs text-text-muted">{text("桌宠可见且有键鼠活动时累计；相邻活动间隔超过 30 秒不计入。每 10 分钟一次机会，10% 获得装扮，连续 15 次未中后下一次必得；未中 +1 碎片，重复 +4 碎片。每日最多 12 次，成就不受限。进度约每分钟保存。", "Activity counts while the pet is visible; gaps over 30 seconds do not count. One opportunity per 10 active minutes, 10% accessory chance, guaranteed after 15 misses. Miss: +1 fragment; duplicate: +4. Up to 12 opportunities per day; achievements are uncapped. Progress saves about once a minute.")}</p>
      </div>
      <div className="flex flex-wrap gap-2">{[0, 1, 2].map(index => <div className="flex items-center gap-1" key={index}><span className="text-xs">{text("搭配", "Outfit")} {index + 1}</span><button className={button} disabled={busy} onClick={() => void act({ kind: "save_preset", index })}>{text("保存", "Save")}</button><button className={button} disabled={busy} onClick={() => void act({ kind: "load_preset", index })}>{text("穿上", "Wear")}</button></div>)}</div>
      <label className="flex items-center justify-between text-xs">{text("吐槽语气", "Chatter tone")}<select className="rounded-md bg-surface-hover border border-border-default p-1" value={w.tone} disabled={busy} onChange={event => void act({ kind: "tone", tone: event.target.value as PetTone })}><option value="mixed">{text("轻松混合", "Mixed")}</option><option value="gentle">{text("温和陪伴", "Gentle")}</option><option value="snarky">{text("俏皮吐槽", "Snarky")}</option></select></label>
      <p className="text-2xs text-text-muted">{text("话题会结合当前装扮的职业、符文和场景；语气单独选择。所有装扮仅用于桌宠，不代表游戏内掉落。", "Topics follow equipped class, rune and scene tags; tone is independent. All accessories are for the desktop pet, not in-game loot.")}</p>
    </>}
  </section>;
}

