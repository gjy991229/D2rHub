/** Editorial data: topic × tone, independent of ownership, rarity or whole skins.
 * Lines are fictional chatter, never simulated telemetry or unlock receipts.
 * Add topics here and attach matching tags to catalog items; no renderer edits.
 */
export type ChatterTone = "gentle" | "snarky";
export interface ChatterLine { id: string; topic: string; tone: ChatterTone; zh: string; en: string }
type Pair = readonly [string, string];
interface Topic { gentle: Pair[]; snarky: Pair[] }
const topics: Record<string, Topic> = {
  common: {
    gentle: [
      ["今天也一起在庇护之地走走。", "Another little journey through Sanctuary together."],
      ["背包可以清空，好心情记得留下。", "Clear the inventory, keep the good mood."],
      ["喝口水吧，我替你守着桌面。", "Have some water. I'll watch the desktop."],
      ["想休息就休息，衣柜进度不会跑。", "Take a break whenever you like. Your wardrobe progress stays."],
      ["普通的一天，也值得配一段鼓点。", "An ordinary day deserves a little drumbeat too."],
      ["今天的目标：玩得开心，肩膀放松。", "Today's goal: have fun and relax those shoulders."],
      ["最好的搭配，是你自己喜欢的那套。", "The best outfit is the one you like."],
      ["把喜欢的东西留下，不必每格都塞满。", "Keep what you love. Not every slot needs filling."],
      ["好运来的时候记得叫我，我也想看看。", "Call me when luck arrives. I'd love to see it."],
      ["你忙你的，我敲我的。", "You do your thing; I'll keep the beat."],
      ["今天没有大惊喜，也可以有小满足。", "A quiet day can still bring small joys."],
      ["路可以慢慢走，猫咪一直在这里。", "Take the road at your own pace. I'm here."],
    ], snarky: [
      ["背包管理，庇护之地真正的终局玩法。", "Inventory management: Sanctuary's real endgame."],
      ["这不是囤积，这是给未来的自己出难题。", "That isn't hoarding. It's a puzzle for your future self."],
      ["装备可以换，‘再来一把’这句台词很难换。", "Gear changes. 'One more run' never does."],
      ["我负责打节拍，你负责和仓库讨价还价。", "I'll keep time while you negotiate with the stash."],
      ["每次整理仓库，都像在考古自己的希望。", "Sorting the stash is archaeology for old hopes."],
      ["‘以后可能有用’，仓库听了都想扩建。", "'Might need it later.' Even the stash wants an extension."],
      ["你说最后一趟的时候，巴尔已经续了杯茶。", "Baal refilled his tea when you said 'last run'."],
      ["真正稀有的东西，是背包里连着的四个空格。", "The rarest find: four adjacent empty inventory slots."],
      ["今天的欧气先欠着，猫粮可不能赊账。", "Luck can arrive later. Dinner is due now."],
      ["整理五分钟，纠结半小时，熟悉的流程。", "Five minutes sorting, thirty minutes deciding. Classic."],
      ["我已经毕业了，专业是陪你纠结底材。", "I graduated in Advanced Base-Item Indecision."],
      ["猫爪没有快捷键冲突，只有罐头优先级。", "My paws have no shortcut conflicts, only food priorities."],
    ],
  },
  camp: { gentle: [
    ["恰西的铁砧声，听起来也很有节奏。", "Charsi's anvil has a rhythm of its own."],
    ["凯恩讲故事的时候，猫咪会认真坐好。", "I sit very politely for Cain's stories."],
    ["营地布帽戴好了，今天从小冒险开始。", "Camp cap on. Time for a small adventure."],
    ["先在营地坐一会儿，下一段路不着急。", "Rest in camp a moment. The road can wait."],
  ], snarky: [
    ["基德的笑容里，藏着我的预算表。", "Gheed's smile has my budget hidden inside."],
    ["凯恩鉴定装备，我鉴定午饭。分工明确。", "Cain identifies items. I identify lunch. Clear roles."],
    ["恰西：欢迎再来。仓库：求你别买。", "Charsi: come again. Stash: please stop shopping."],
    ["圆眼镜一戴，每个蓝装都像研究课题。", "With these glasses, every blue item is a research project."],
  ] },
  cat: { gentle: [
    ["铃铛响一下，给今天加一点轻快。", "One bell chime to brighten the day."],
    ["猫咪的配装原则：不影响伸懒腰。", "My gearing rule: leave room for a stretch."],
    ["你敲键盘，我敲鼓，配合刚刚好。", "You type, I drum. A lovely little duet."],
    ["这条围巾很暖，适合守着你发呆。", "This scarf is warm enough for a quiet day beside you."],
  ], snarky: [
    ["铃铛不是警报，是猫粮余额提醒。", "That's no alarm. It's a low-cat-food notification."],
    ["全身六个槽位，居然没有罐头位。", "Six outfit slots and none for canned food."],
    ["这双爪子不加攻速，但加可爱。", "These paws grant no attack speed, just cuteness."],
    ["今日最佳队友：没有踩到键盘的我。", "Best teammate today: me, for staying off the keyboard."],
  ] },
  journey: { gentle: [
    ["旅人兜帽记录的是陪伴，不是赶路。", "The travel hood remembers company, not speed."],
    ["不用连续签到，再见到你就很高兴。", "No streak required. I'm simply glad you're back."],
    ["铜色项链慢慢有了旧朋友的温度。", "The bronze charm is warming into an old friendship."],
    ["长路披着星光，我们慢慢往前走。", "A starlit cape for a journey at our own pace."],
  ], snarky: [
    ["旅行计划：出门冒险，三分钟后想家。", "Travel plan: adventure, then homesickness in three minutes."],
    ["兜帽有了，方向感还在等掉落。", "Hood acquired. Sense of direction still pending."],
    ["星光披风不包邮，也不包传送。", "The starlit cape includes neither shipping nor teleportation."],
    ["这条路走得很长，主要是我腿短。", "This road feels long. Mostly because my legs are short."],
  ] },
  sorceress: { gentle: [
    ["冰封球的灵感，也许来自一团毛线。", "Perhaps Frozen Orb began with a ball of yarn."],
    ["宝石头环戴好了，今天研究一点小魔法。", "Circlet on. Time to study a little magic."],
    ["火光适合照路，也适合围着休息。", "Firelight is good for finding roads and taking rests."],
    ["我的三系魔法：暖气、冰箱和静电。", "My three elements: heater, fridge and static fur."],
  ], snarky: [
    ["传送术练了半天，最熟练的落点还是饭碗。", "My best-practiced teleport destination is still the food bowl."],
    ["施法档位算得很准，睡午觉从不读条。", "Perfect cast calculations. Naps need no cast time."],
    ["陨石术暂时停课，花盆还没赔完。", "Meteor lessons are paused until the flowerpots are paid for."],
    ["法力见底时，每瓶蓝药都像年终奖。", "At low mana, every blue potion feels like a bonus."],
  ] },
  amazon: { gentle: [
    ["羽饰额带轻一点，风就多一点。", "A light feather band leaves more room for the breeze."],
    ["今天练习瞄准，目标是一团毛线。", "Today's target practice involves a ball of yarn."],
    ["长矛和弓都很帅，猫咪先练站稳。", "Spears and bows look grand. I'll start with standing steady."],
    ["亚马逊的行囊里，也该留一点午餐的位置。", "An Amazon's pack should have a little room for lunch."],
  ], snarky: [
    ["标枪还没出手，羽毛先被我玩秃了。", "The javelin hasn't flown. I've already played the feather bare."],
    ["箭袋很满，方向感很空。", "Full quiver, empty sense of direction."],
    ["远程的好处，是离加班的铁匠也远一点。", "The perk of range: being farther from the overworked smith."],
    ["瞄准巴尔之前，我先瞄准下班时间。", "Before aiming at Baal, I aim for quitting time."],
  ] },
  assassin: { gentle: [
    ["影爪护腕系好了，轻轻落爪。", "Shadow wraps tied. A soft landing for every paw."],
    ["陷阱摆整齐，桌面也摆整齐。", "Neat traps, neat desktop."],
    ["影子陪你赶路，猫咪陪你打字。", "Shadows keep you company on the road; I do while you type."],
    ["收好锋芒，今天也可以安静地陪伴。", "Even sharp claws can offer quiet company."],
  ], snarky: [
    ["我的陷阱叫纸箱，效果是把自己关进去。", "My trap is a cardboard box. It catches me."],
    ["影子大师负责打怪，我负责装大师。", "Shadow Master fights. I look masterful."],
    ["连招很复杂，开罐头只需要一招。", "Combos are complex. Opening dinner needs one move."],
    ["潜行失败：铃铛先替我报了名字。", "Stealth failed. The bell introduced me first."],
  ] },
  barbarian: { gentle: [
    ["战吼角盔戴稳，气势从鼓点开始。", "Horned helm steady. Let the beat lead the charge."],
    ["大声鼓劲，也记得轻声说辛苦了。", "Cheer loudly, but remember a quiet thank-you."],
    ["双持猫爪，今天依然认真工作。", "Dual-wielding paws, still doing an honest day's work."],
    ["山顶的风很冷，围巾也算战备。", "Mountain winds are cold. A scarf counts as preparation."],
  ], snarky: [
    ["战吼准备好了：开——饭——啦！", "Battle cry ready: DINNER!"],
    ["旋风练习暂停，猫咪有点晕。", "Whirlwind practice paused. Cat is dizzy."],
    ["双持的终极理想，是一爪一个罐头。", "The dream of dual wielding: a can of food in each paw."],
    ["气势拉满，声音出来是喵。", "Maximum intimidation. The sound is still 'meow'."],
  ] },
  paladin: { gentle: [
    ["圣徽披风铺好，给队友留个位置。", "Heraldic cape spread out, with room for a friend."],
    ["愿今天的旅途，有一点安稳的光。", "May today's road have a little steady light."],
    ["守护有很多种，安静陪着也是一种。", "There are many ways to guard someone. Quiet company is one."],
    ["盾牌朝外，温柔留给队友。", "Shield outward, kindness toward the party."],
  ], snarky: [
    ["锤子转得很圆，我的上班路线也一样。", "The hammer travels in circles. So does my workday."],
    ["圣光照着桌面，照不进背包里的乱。", "Holy light reaches the desk, but not that inventory mess."],
    ["光环切得很勤，午饭依然需要自己点。", "All those aura switches still won't order lunch."],
    ["披风很有威严，里面藏着一只打哈欠的猫。", "A majestic cape hides a yawning cat."],
  ] },
  druid: { gentle: [
    ["鹿角沾着林地的风，今天走慢一点。", "Antlers carry a forest breeze. Let's walk slowly today."],
    ["如果能变成熊，就给午睡多留点地方。", "If I could become a bear, I'd reserve a larger nap spot."],
    ["橡树的影子下，适合听一点鼓声。", "An oak's shade is a fine place for a little drumming."],
    ["狼群有伙伴，桌面上也有我。", "The wolf pack has company. Your desktop has me."],
  ], snarky: [
    ["变狼计划失败，还是想追逗猫棒。", "Wolf transformation failed. Still chasing the cat toy."],
    ["召唤乌鸦之前，先谈好谁负责擦桌子。", "Before summoning ravens, agree on who cleans the desk."],
    ["自然之力很强，但猫薄荷更难抵抗。", "Nature is powerful. Catnip is harder to resist."],
    ["鹿角不是衣架，围巾请走颈部槽位。", "Antlers aren't a coat rack. Scarves go in the neck slot."],
  ] },
  necromancer: { gentle: [
    ["骨纹项坠只是装饰，猫咪的心还是暖的。", "The bone pendant is decoration. My heart is still warm."],
    ["小幽灵也想有个固定的队伍。", "Even a little ghost wants a regular party."],
    ["暗处也可以有安静的陪伴。", "Quiet company belongs in the shadows too."],
    ["今天不召唤大军，只召唤好心情。", "No army today. Just summoning a good mood."],
  ], snarky: [
    ["骷髅负责冲锋，法师负责数人头。", "Skeletons charge. Their boss counts heads."],
    ["召唤队伍越来越大，过门越来越慢。", "A growing army, a shrinking doorway."],
    ["小幽灵说它不吃饭。很好，预算安全了。", "The ghost says it doesn't eat. Excellent news for the budget."],
    ["骨牢搭得很快，装修许可证还没批。", "Bone Prison went up fast. The building permit hasn't."],
  ] },
  warlock: { gentle: [
    ["契约魔典翻开了，第一页写着照顾自己。", "The pact tome opens. Page one says: take care of yourself."],
    ["禁忌知识很深，今天先读一小页。", "Forbidden knowledge runs deep. One page today is enough."],
    ["魔典悬在身边，陪你慢慢研究新旅途。", "The floating tome keeps us company on a new journey."],
    ["黑暗魔法之外，也留一点日常的温柔。", "Make room for everyday kindness alongside dark magic."],
  ], snarky: [
    ["契约写了三百页，猫咪只看见包不包饭。", "A three-hundred-page pact. I only checked for meal coverage."],
    ["恶魔还没签字，猫先按了个爪印。", "The demon hasn't signed. The cat already stamped a paw."],
    ["魔典会悬浮，待办事项为什么不会自己消失？", "The tome can float. Why can't the to-do list vanish?"],
    ["召唤仪式准备就绪，缺一支能写字的笔。", "Summoning ritual ready. Missing: a pen that works."],
  ] },
  rune: { gentle: [
    ["符文石只是桌面装饰，好看就值得留下。", "Rune stones are desktop ornaments. Looking lovely is enough."],
    ["每道刻痕，都像一段很小的冒险故事。", "Every engraved line looks like a tiny adventure story."],
    ["让符文陪着鼓点，今天多一点仪式感。", "A rune beside the beat adds a little ceremony to the day."],
    ["真正的宝藏，也可以是一起度过的时间。", "Time spent together can be a treasure too."],
  ], snarky: [
    ["装饰符文不会掉进游戏，猫咪的物流还没跨界。", "Decorative runes don't enter the game. Cat logistics isn't interdimensional."],
    ["这块石头看起来很贵，实际职业是陪我站岗。", "An expensive-looking stone with a job as my desk companion."],
    ["符文石没有孔，别拿电钻过来。", "No sockets on this ornament. Put the drill away."],
    ["我盯着符文看了半天，它也没帮我写一个字。", "I've stared at this rune for ages. It hasn't typed a word."],
  ] },
  lo: { gentle: [
    ["罗 Lo 的暖色，像营地里的一小块火光。", "Lo's warm color is a little piece of campfire."],
    ["今天让罗符文陪在右边，安静又稳当。", "Lo is keeping quiet, steady company on the right today."],
    ["石头很小，喜欢它的理由可以很大。", "A tiny stone can hold a big reason to smile."],
  ], snarky: [
    ["罗已经站岗了，刚毅的只有我等饭的决心。", "Lo is on duty. My true Fortitude is waiting for dinner."],
    ["悔恨是什么？是整理仓库时又想起那件底材。", "Grief is remembering that base item while sorting the stash."],
    ["这块罗不会让鼠标增伤，只会让桌面增帅。", "This Lo buffs desktop style, not mouse damage."],
  ] },
  ber: { gentle: [
    ["贝 Ber 收好了，让期待在桌面上有个位置。", "Ber is safe. A little place for hope on the desktop."],
    ["贝符文的刻痕，猫咪已经认真描了三遍。", "I've carefully traced Ber's markings three times."],
    ["不急着凑齐所有东西，先欣赏手里的这一块。", "No rush to collect everything. Enjoy the stone you have."],
  ], snarky: [
    ["贝在身边，猫粮预算依然归零。", "Ber beside me. Cat-food budget still zero."],
    ["这块贝不用藏仓库，猫咪已经聘它当保安。", "No need to stash Ber. I've hired it as security."],
    ["无限的是愿望，有限的是背包。", "Infinity describes the wish list, not the inventory."],
  ] },
  jah: { gentle: [
    ["乔 Jah 在身边，像把一个小愿望摆上桌。", "Jah beside me feels like a little wish on the desk."],
    ["乔符文的光不刺眼，刚好照着这段陪伴。", "Jah's gentle glow suits this quiet company."],
    ["今天不用赶路，就让乔在这里陪一会儿。", "No need to hurry today. Let Jah keep us company."],
  ], snarky: [
    ["乔到位了，传送到饭碗仍需猫咪步行。", "Jah is here. Getting to dinner still requires walking."],
    ["谜团是什么？是我明明吃过，为什么还饿。", "The Enigma: I've eaten, so why am I still hungry?"],
    ["乔贝都想要，先别把中间那枚小符文给忘了。", "Dreaming of Jah and Ber? Don't forget the little rune between them."],
  ] },
};
export const CHATTER_LINES: readonly ChatterLine[] = Object.entries(topics).flatMap(([topic, pool]) =>
  (["gentle", "snarky"] as const).flatMap(tone => pool[tone].map(([zh, en], index) => ({ id: `${topic}.${tone}.${index}`, topic, tone, zh, en }))));

/** Real lifecycle notices use their own event pool, never an item-quality roll. */
export const PET_EVENT_LINES = {
  launchSuccess: [
    ["启动成功，旅途顺利！", "Launch complete. Have a good journey!"],
    ["启动完成，猫咪已经准备好陪你出发。", "Launch complete. Your companion is ready."],
    ["出发准备完成，记得带上好心情。", "Ready to set out. Bring a good mood along."],
  ],
  launchFailure: [
    ["启动未完成，任务状态里有具体原因。", "Launch failed. The task status has details."],
    ["这次没能顺利启动，先看看任务提示。", "This launch didn't finish. Check the task details."],
    ["启动遇到问题，猫咪陪你一起看任务状态。", "A launch issue came up. Let's check the task status."],
  ],
} satisfies Record<string, Pair[]>;
