// Captured from the real React components with browser-only sample data.
// Preserve the source PNGs; crop, emphasis and labels are rendered as SVG/HTML.
const GUIDE_FIGURES = {
  "recognition": {
    "title": "先选监听账号，再准备识别 Mod",
    "file": "recognition.png",
    "crop": [
      414,
      121,
      709,
      536
    ],
    "note": "截图展示“待准备 Mod”的示例状态。开关打开不等于已开始捕获；请以运行状态为准。",
    "marks": [
      [
        570,
        415,
        150,
        38,
        "选择实际刷图的账号"
      ],
      [
        632,
        230,
        87,
        38,
        "缺少识别 Mod 时从这里准备"
      ],
      [
        424,
        566,
        298,
        82,
        "准备并重启后，在这里看捕获状态与地点信号"
      ]
    ]
  },
  "mod": {
    "title": "Mod 加工：按这四处依次设置",
    "file": "mod-processing.png",
    "crop": [
      414,
      125,
      712,
      548
    ],
    "note": "此图以已有 Mod 为来源，演示同时准备两项能力。只做识别或跟房时，按需选择对应功能。",
    "marks": [
      [
        427,
        217,
        260,
        34,
        "选加工目标账号"
      ],
      [
        427,
        510,
        260,
        146,
        "选原版或已有 Mod 来源"
      ],
      [
        724,
        197,
        388,
        106,
        "选择声纹识别 / 局内房间工具"
      ],
      [
        724,
        515,
        388,
        82,
        "填写新名称，然后加工并应用"
      ]
    ]
  },
  "participants": {
    "title": "跟房准备：主号、小号、Mod 都要对应",
    "file": "room-participants.png",
    "crop": [
      414,
      255,
      707,
      412
    ],
    "note": "账号名称和“已就绪”均为示例。向下滚动，继续为每个小号确认 Mod，不要只检查主号。",
    "marks": [
      [
        428,
        361,
        267,
        39,
        "指定唯一的主账号"
      ],
      [
        428,
        480,
        214,
        40,
        "勾选需要跟随的小号"
      ],
      [
        668,
        597,
        178,
        63,
        "逐个账号确认房间工具成品"
      ]
    ]
  },
  "method": {
    "title": "操作方案与跟随触发，是两项不同设置",
    "file": "room-method.png",
    "crop": [
      414,
      300,
      707,
      361
    ],
    "note": "截图选择的是后台按键。首次建议手动跟随，熟悉后再切自动。",
    "marks": [
      [
        428,
        380,
        172,
        39,
        "后台同步预填，创建时保持主号前台"
      ],
      [
        428,
        502,
        172,
        39,
        "首次先手动跟随"
      ],
      [
        772,
        502,
        172,
        39,
        "设置小号派发方式"
      ],
      [
        428,
        585,
        456,
        60,
        "分别设置建房与跟随快捷键"
      ]
    ]
  },
  "paths": {
    "title": "只配置你实际使用的客户端",
    "file": "paths.png",
    "crop": [
      414,
      68,
      708,
      625
    ],
    "note": "路径均为示例，不要照抄。国服与国际服分开选择；存档目录服务于账号独立画质配置，缺少 Settings.json 时按页面提示处理。",
    "marks": [
      [
        424,
        266,
        326,
        128,
        "国服玩家：选择国服游戏与存档位置"
      ],
      [
        424,
        565,
        326,
        129,
        "国际服玩家：使用国际服目录"
      ],
      [
        783,
        254,
        326,
        113,
        "网页 Token 流程使用隔离浏览器"
      ]
    ]
  },
  "init": {
    "title": "添加账号：从本地昵称开始",
    "file": "account-init.png",
    "crop": [
      831,
      238,
      433,
      245
    ],
    "note": "这里只填写用于辨认账号的昵称，不是战网密码。后续按向导选择区服、认证方式并完成登录。",
    "marks": [
      [
        850,
        361,
        394,
        46,
        "先填容易区分的昵称，例如主号或小号1"
      ],
      [
        850,
        416,
        394,
        36,
        "点击下一步，按顶部顺序继续初始化"
      ]
    ]
  },
  "accounts": {
    "title": "多开账号各自保存配置",
    "file": "accounts.png",
    "crop": [
      414,
      68,
      712,
      380
    ],
    "note": "这是设置中心的账号配置页，不是游戏启动按钮。完成初始化后，请回主界面账号卡片启动；此图账号与状态均为示例。",
    "marks": [
      [
        415,
        68,
        246,
        281,
        "先选择需要配置的账号"
      ],
      [
        903,
        189,
        209,
        66,
        "确认当前账号实际选用的 Mod"
      ],
      [
        416,
        355,
        244,
        91,
        "未初始化的账号要先完成认证"
      ]
    ]
  },
  "modules": {
    "title": "需要哪项扩展，就打开哪张模块卡片",
    "file": "modules.png",
    "crop": [
      413,
      451,
      491,
      243
    ],
    "note": "图中模块已添加，因此显示“打开”。首次使用在相应卡片添加模块；不要点击“卸载”。添加软件模块后仍需准备对应游戏 Mod。",
    "marks": [
      [
        423,
        462,
        219,
        211,
        "识别统计：添加后打开此模块"
      ],
      [
        672,
        462,
        220,
        211,
        "自动跟房：添加后打开此模块"
      ]
    ]
  },
  "rules": {
    "title": "房名由前缀、序号和位数一起决定",
    "file": "room-rules.png",
    "crop": [
      414,
      302,
      706,
      304
    ],
    "note": "示例 chaos- + 27 + 3 位得到 chaos-027。你可以按正文填写 hub-001；密码也是示例。截图中的旧版聊天键扫描入口已移除，无需操作。",
    "marks": [
      [
        427,
        370,
        677,
        57,
        "填写房名开头、下一个序号和序号位数"
      ],
      [
        1016,
        313,
        86,
        30,
        "核对下次房名预览"
      ],
      [
        427,
        438,
        265,
        55,
        "密码可选；留空就是无密码房间"
      ],
      [
        955,
        548,
        152,
        39,
        "后台方案在此扫描键位，之后重启游戏"
      ]
    ]
  },
  "auto": {
    "title": "切换自动跟随，并留足主号载入时间",
    "file": "room-auto.png",
    "crop": [
      414,
      329,
      706,
      277
    ],
    "note": "5 秒只是示例值，应覆盖你机器上主号的实际载入时间。切换后仍需由你触发主号建房；没有在本机执行跟房任务。",
    "marks": [
      [
        515,
        371,
        85,
        39,
        "切到自动跟随"
      ],
      [
        427,
        451,
        143,
        58,
        "设置建房后等待秒数"
      ],
      [
        770,
        347,
        176,
        64,
        "选择小号派发方式"
      ],
      [
        770,
        451,
        132,
        64,
        "随后使用建房快捷键触发新房"
      ]
    ]
  }
};
const GUIDE_STEP_FIGURES = {
  paths: ['paths'], first: ['init'], second: ['init', 'accounts'],
  'audio-install': ['modules', 'recognition'], 'audio-mod': ['mod'], 'combined-mod': ['mod', 'accounts'],
  'room-install': ['modules', 'participants'], 'room-mod': ['participants', 'mod'],
  'room-method': ['method'], 'room-rules': ['rules', 'method'], 'room-auto': ['auto']
};
function guideStepFigureMarkup(stepId) {
  const keys = GUIDE_STEP_FIGURES[stepId];
  if (!keys) return '';
  const picker = keys.length > 1 ? `<div class="guide-gallery-picker" aria-label="本步图示顺序">${keys.map((key,index)=>`<button type="button" data-gallery-key="${key}" aria-pressed="${index === 0}">${index + 1}. ${GUIDE_FIGURES[key].title}</button>`).join('')}</div>` : '';
  return `<div class="guide-gallery">${picker}<div class="guide-gallery-content">${guideFigureMarkup(keys[0])}</div></div>`;
}
function bindGuideGalleries(container) {
  bindGuideFigures(container);
  container.querySelectorAll('.guide-gallery').forEach(gallery => {
    gallery.querySelectorAll('[data-gallery-key]').forEach(button => button.onclick = () => {
      gallery.querySelectorAll('[data-gallery-key]').forEach(item => item.setAttribute('aria-pressed', String(item === button)));
      const content = gallery.querySelector('.guide-gallery-content');
      content.innerHTML = guideFigureMarkup(button.dataset.galleryKey);
      bindGuideFigures(content);
    });
  });
}

function guideFigureMarkup(key, base = 'guide-images/') {
  const figure = GUIDE_FIGURES[key];
  if (!figure) return '';
  const [x,y,w,h] = figure.crop;
  return `<figure class="guide-figure" data-figure="${key}">
    <figcaption><strong>${figure.title}</strong><span>真实组件截图 · 示例账号与状态</span></figcaption>
    <svg class="guide-picture" viewBox="${x} ${y} ${w} ${h}" role="img" aria-label="${figure.title}，编号对应下方操作说明">
      <defs><clipPath id="crop-${key}"><rect x="${x}" y="${y}" width="${w}" height="${h}" /></clipPath></defs>
      <g clip-path="url(#crop-${key})">
      <image href="${base}${figure.file}" width="1280" height="720" />
      <path class="guide-shade" fill="rgba(12,18,28,.40)" fill-rule="evenodd" d="" />
      ${figure.marks.map(([mx,my,mw,mh,label],i)=>`<g class="guide-mark" data-mark="${i}"><rect x="${mx}" y="${my}" width="${mw}" height="${mh}" rx="5" /><circle cx="${mx+10}" cy="${my}" r="12" /><text x="${mx+10}" y="${my+1}">${i+1}</text></g>`).join('')}
      </g>
    </svg>
    <div class="guide-image-actions"><span>点编号，单独强调对应位置</span><button type="button" data-highlight="all" aria-pressed="true">显示全部</button><a href="${base}${figure.file}" target="_blank" rel="noopener">打开原图 ↗</a></div>
    <div class="guide-legend">${figure.marks.map((mark,i)=>`<button type="button" data-highlight="${i}" aria-pressed="false"><b>${i+1}</b><span>${mark[4]}</span></button>`).join('')}</div>
    <p class="guide-image-note">${figure.note}</p>
  </figure>`;
}
function bindGuideFigures(container) {
  container.querySelectorAll('[data-figure]').forEach(element => {
    const figure = GUIDE_FIGURES[element.dataset.figure];
    element.querySelectorAll('[data-highlight]').forEach(button => button.onclick = () => {
      const value = button.dataset.highlight;
      element.querySelectorAll('[data-highlight]').forEach(item => item.setAttribute('aria-pressed', String(item === button)));
      element.querySelectorAll('[data-mark]').forEach(mark => mark.classList.toggle('is-muted', value !== 'all' && mark.dataset.mark !== value));
      let path = '';
      if (value !== 'all') {
        const [x,y,w,h] = figure.crop, [mx,my,mw,mh] = figure.marks[Number(value)];
        path = `M${x},${y}h${w}v${h}h-${w}z M${mx},${my}h${mw}v${mh}h-${mw}z`;
      }
      element.querySelector('.guide-shade').setAttribute('d', path);
    });
  });
}
