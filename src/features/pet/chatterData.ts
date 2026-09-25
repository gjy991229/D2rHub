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
      ["窗外有风，桌边有猫，今天慢慢来。", "A breeze outside, a cat beside you. Take your time today."],
      ["偶尔看看远处，让眼睛也散个步。", "Look into the distance sometimes. Let your eyes wander too."],
      ["没做完的事，留一点给明天也可以。", "It's all right to leave a little for tomorrow."],
      ["不一定要有收获，同行本身就很好。", "We don't need a big find. The company is enough."],
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
      ["攻略收藏了一百篇，出门还是先找传送点。", "A hundred saved guides. Still looking for the waypoint first."],
      ["仓库不是满了，是梦想摆得太密。", "The stash isn't full. The dreams are packed too tightly."],
      ["你的待办叫任务，我的待办叫待喂。", "Your list says to-do. Mine says to-feed."],
      ["这把椅子的隐藏属性，是让人忘记起来。", "This chair's hidden stat makes you forget to stand up."],
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
  treasure: { gentle: [
    ["罗盘轻轻转，今天去看看没走过的路。", "The compass turns softly. Let's try a road we haven't taken."],
    ["风镜擦亮了，小小的风景也值得看清。", "Goggles polished. Even little sights deserve a clear view."],
    ["宝箱之外，沿途的故事也值得收藏。", "Beyond the chests, there are stories worth collecting."],
    ["地图留一角空白，给下一次好奇心。", "Leave a blank corner on the map for our next curiosity."],
  ], snarky: [
    ["罗盘指向北方，肚子指向厨房。", "The compass points north. My stomach points to the kitchen."],
    ["寻宝风镜戴上了，丢的鼠标指针还是找不到。", "Explorer goggles on. Still can't find the mouse pointer."],
    ["宝藏还没找到，回程路已经忘了。", "No treasure yet. Already forgot the way back."],
    ["地图上的叉，可能只是我踩过的爪印。", "That X on the map might just be my paw print."],
  ] },
  stargazer: { gentle: [
    ["把一小片星空，留在你的桌面上。", "A little patch of stars, right here on your desktop."],
    ["弯月挂在帽尖，今晚的梦可以轻一点。", "A crescent on my hat, a softer dream tonight."],
    ["极光披在肩上，安静也是一种颜色。", "An aurora on my shoulders. Quiet has a color too."],
    ["看星星不必数清楚，喜欢就多看一会儿。", "No need to count the stars. Just enjoy them a little longer."],
  ], snarky: [
    ["观星单镜很专业，主要用来找夜宵星。", "A professional monocle, mostly for spotting the midnight-snack star."],
    ["帽尖离星星近了一点，离门框也近了一点。", "This hat brings me closer to the stars. And the doorframe."],
    ["我的星座是饭碗座，全天都很活跃。", "My constellation is the Food Bowl. Active around the clock."],
    ["许愿池暂时关闭，猫咪只接收罐头形式的流星。", "Wishes are closed. Shooting stars must arrive as canned food."],
  ] },
  garden: { gentle: [
    ["花冠不用浇水，好心情可以慢慢养。", "This flower crown needs no water. Good moods can grow slowly."],
    ["叶脉像小路，弯弯绕绕也能到家。", "Leaf veins look like paths. Winding roads can still lead home."],
    ["给桌面添一点绿，像给今天开一扇窗。", "A touch of green on the desk feels like opening a window."],
    ["花开有自己的时间，我们也一样。", "Flowers bloom in their own time. So do we."],
  ], snarky: [
    ["草叶花冠很新鲜，但请不要往猫头上浇水。", "The crown looks fresh. Please don't water the cat."],
    ["这件叶子披风，暂时还不支持光合作用。", "This leaf cape doesn't support photosynthesis yet."],
    ["园艺第一课：花盆不能兼任猫床。好难。", "Gardening lesson one: pots aren't cat beds. Tough lesson."],
    ["我在研究自然，具体项目是草为什么这么好咬。", "I'm studying nature. Specifically, why grass is so chewable."],
  ] },
  tea: { gentle: [
    ["热可可慢慢凉，我们慢慢聊。", "Let the cocoa cool while we take our time talking."],
    ["领结系好了，给平常的一天一点认真。", "Bow tie tied. A little care for an ordinary day."],
    ["杯子里装着暖意，桌边留着你的位置。", "Warmth in the cup, a place for you beside the desk."],
    ["茶歇不需要理由，歇一会儿就很好。", "A tea break needs no reason. Resting is enough."],
  ], snarky: [
    ["领结像上班，表情像等下班。", "The bow tie says work. My face says home time."],
    ["这杯可可是摆件，真正的下午茶请另行投喂。", "The cocoa is a prop. Actual afternoon snacks sold separately."],
    ["我可以陪你加班，但点心要按双份算。", "I can keep you company overtime. Snacks count double."],
    ["优雅喝茶的第一步，是别把胡须泡进去。", "Elegant tea, step one: keep your whiskers out of the cup."],
  ] },
  workshop: { gentle: [
    ["铜扣手套戴好，认真做好手边的小事。", "Brass mitts on. Let's give the little things some care."],
    ["铁砧有铁砧的节奏，猫爪有猫爪的节奏。", "Anvils have their rhythm. Paws have theirs."],
    ["打磨一点点，喜欢的东西就更亮一点。", "A little polishing makes a favorite thing shine brighter."],
    ["工具收好再休息，明天开工会轻松些。", "Put the tools away before resting. Tomorrow will start easier."],
  ], snarky: [
    ["手套很像大师，作品很像第一次。", "Master-crafter gloves. First-attempt results."],
    ["修理费还没算，猫咪先收一个摸头。", "Before the repair bill, one head pat as a deposit."],
    ["镶孔这活交给专家，我只擅长给纸箱打洞。", "Leave sockets to the experts. I specialize in cardboard holes."],
    ["锤子没拿稳，气势倒是敲得很响。", "My hammer grip is shaky. My confidence is very loud."],
  ] },
  alchemy: { gentle: [
    ["星蓝药瓶里，装着一点安静的想象。", "A little quiet imagination in a starlight-blue bottle."],
    ["今天的配方：一点耐心，加一点好奇。", "Today's recipe: a little patience and a little curiosity."],
    ["护腕扣好了，慢慢试也是一种进步。", "Cuffs fastened. Trying slowly is progress too."],
    ["瓶里的微光，刚好陪你读完这一页。", "A tiny glow in the bottle to keep you company through this page."],
  ], snarky: [
    ["炼金配方保密，主要是我忘了刚才放了什么。", "The recipe is secret. Mostly because I forgot what I added."],
    ["药瓶是装饰，不能给加班补法力。", "The bottle is decorative. It won't refill your overtime mana."],
    ["我试着合成耐心，结果先把耐心用完了。", "I tried crafting patience. Ran out of patience first."],
    ["炼金护腕已就位，量杯被我拿去装小鱼干了。", "Alchemy cuffs ready. The measuring cup is full of fish treats."],
  ] },
  voyage: { gentle: [
    ["小纸船准备好了，今天驶向一点新鲜事。", "The paper boat is ready to sail toward something new."],
    ["海风把领巾吹起来，也把烦恼吹远一点。", "The sea breeze lifts my scarf and carries worries away."],
    ["船长帽戴稳，我们慢慢找自己的航线。", "Captain hat steady. Let's find a route at our own pace."],
    ["不必每次都远航，靠岸休息也很好。", "Not every day needs a voyage. Resting in port is lovely too."],
    ["灯塔还亮着，晚归也有方向。", "The lighthouse is still lit, even for a late return."],
    ["把今天的小愿望折进纸船里。", "Fold a little wish into today's paper boat."],
  ], snarky: [
    ["船长的第一条命令：甲板上禁止偷吃我的鱼。", "Captain's first order: no stealing my fish on deck."],
    ["海盗眼罩很威风，找饭碗时先摘掉。", "A fearsome eye patch. Off it comes when finding dinner."],
    ["这艘纸船载得动梦想，载不动你的仓库。", "This paper boat holds dreams, but not your entire stash."],
    ["航海日志：风平浪静，猫咪晕键盘。", "Ship's log: calm seas, keyboard-sick cat."],
    ["我的藏宝图只有一个叉，在冰箱上。", "My treasure map has one X. It's on the fridge."],
    ["水手结打好了，解开可能要等下个赛季。", "Sailor knot tied. Untying it may take another season."],
  ] },
  winter: { gentle: [
    ["绒毛护腕暖暖的，今天也轻轻落爪。", "Warm fur cuffs for a soft little landing today."],
    ["披风上落一片雪，桌边留一盏灯。", "A snowflake on my cape, a lamp beside your desk."],
    ["天冷的时候，慢一点也很舒服。", "On cold days, a slower pace feels just right."],
    ["把手暖一暖，下一段路再出发。", "Warm your hands before the next stretch of road."],
    ["雪地里的爪印，一步一步都是陪伴。", "Paw prints in the snow, company in every step."],
    ["今天的披风很软，适合裹住一个小哈欠。", "This soft cape is just right for wrapping up a little yawn."],
  ], snarky: [
    ["冰冷强化我不懂，暖气强化我很懂。", "Cold Enchanted? No idea. Heater Enchanted? Expert."],
    ["护腕保暖，不能代替你穿秋裤。", "My cuffs are warm. You still need warm clothes."],
    ["雪花很好看，但罐头请不要冷冻。", "Snowflakes are lovely. Please don't freeze my dinner."],
    ["冬天的终极配装，是被窝加热水袋。", "The ultimate winter build: blanket and hot-water bottle."],
    ["披风上是雪花，键盘上是我的毛。", "Snowflakes on the cape. Cat hair on the keyboard."],
    ["我不是懒，我在给体温做预算。", "I'm not lazy. I'm budgeting body heat."],
  ] },
  festival: { gentle: [
    ["把小小的进步，也当成值得庆祝的事。", "Even little steps deserve a celebration."],
    ["庆典面具戴好了，笑容不用藏起来。", "Festival mask on. No need to hide your smile."],
    ["今天的彩带，为每一次回来飘起来。", "Today's ribbons flutter for every return."],
    ["热闹过后，猫咪还会安静地陪你。", "When the party quiets down, I'll still be here."],
    ["不用等大日子，平常也可以穿得漂亮。", "No need for a special date to dress up nicely."],
    ["披风上的金线，像我们攒下的小片阳光。", "Golden threads on the cape, like sunshine we've saved."],
  ], snarky: [
    ["庆典预算通过了，蛋糕预算还在猫爪审批中。", "Party budget approved. Cake budget awaits paw approval."],
    ["面具负责神秘，我负责暴露吃货本性。", "The mask brings mystery. I reveal my snack obsession."],
    ["彩带别挂鹿角上，那边已经有人预约晒围巾。", "No ribbons on the antlers. They're booked for drying scarves."],
    ["今天盛装出席的活动，是坐着。", "Today's formal occasion: sitting down."],
    ["披风一甩很帅，扫到茶杯就不太帅。", "A cape flourish looks grand, until it hits the teacup."],
    ["庆祝一下：今天还没把领结戴反。", "Let's celebrate: I haven't put my bow tie on backwards yet."],
  ] },
  music: { gentle: [
    ["敲快敲慢都可以，我们有自己的节拍。", "Fast or slow, we have our own rhythm."],
    ["音符精灵在身边，把普通的一刻变成小合奏。", "The music sprite turns an ordinary moment into a little duet."],
    ["休止符也是音乐，休息也是旅途。", "Rests belong in music, and breaks belong in a journey."],
    ["护腕上的音符，记着一起度过的节奏。", "The notes on these wraps remember our shared rhythm."],
    ["不必追着拍子跑，舒服的速度就很好。", "No need to chase the beat. A comfortable pace is enough."],
    ["你留下文字，我留下轻轻的鼓点。", "You leave words. I leave a gentle beat."],
  ], snarky: [
    ["我的保留曲目，是开饭前的即兴独奏。", "My signature piece is the pre-dinner improvised solo."],
    ["音符精灵没有音量键，但至少不会唱跑调。", "The music sprite has no volume button. At least it won't sing off-key."],
    ["打字像演奏，退格键是返场。", "Typing is a performance. Backspace is the encore."],
    ["鼓手已经就位，指挥是一罐没打开的鱼。", "Drummer ready. Conductor: an unopened can of fish."],
    ["猫咪不催你冲次数，爪子也要准点下班。", "No chasing input counts. These paws clock out on time too."],
    ["节奏可以自由，晚饭时间不能自由发挥。", "The rhythm can be free. Dinner time cannot."],
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
