// Captured from the real React components with browser-only sample data.
// Preserve the source PNGs; crop, emphasis and labels are rendered as SVG/HTML.
const GUIDE_FIGURES = {
  "collaboration": {
    "title": "快捷设置：离开时自动保存",
    "file": "room-collaboration.png",
    "crop": [
      480,
      103,
      430,
      591
    ],
    "note": "2026-10-20 真实组件、示例数据。图中为旧版三种模式；当前另有“超级迪亚波罗”，按账号顺序各建独立房间，详见正文。关闭、点击外部或按 Esc 自动保存模式与规则；此图未执行游戏操作。",
    "marks": [
      [
        500,
        300,
        387,
        43,
        "选择默认、拼房主机或拼房跟随"
      ],
      [
        748,
        385,
        140,
        32,
        "复制或粘贴房间规则，内容包含密码"
      ],
      [
        500,
        646,
        387,
        32,
        "离开时自动保存；完整设置会等待保存完成"
      ]
    ],
    "size": [
      1041,
      810
    ]
  },
  "recognition": {
    "title": "先选监听账号，再准备识别 Mod",
    "file": "recognition.png",
    "crop": [
      355,
      68,
      773,
      633
    ],
    "note": "旧版账号模式示例：当前账号模式在 Mod 管理加工、账号或方案成员参数中选用成品；“指定游戏进程”模式仍保留 Mod 选择及准备入口。开关打开不等于已捕获，重启游戏后以运行状态为准。",
    "marks": [
      [
        899,
        363,
        217,
        37,
        "选择实际刷图的账号"
      ],
      [
        366,
        443,
        750,
        38,
        "旧版入口；当前到 Mod 管理加工，并在账号或方案中选用"
      ],
      [
        1076,
        239,
        42,
        25,
        "Mod 准备并重启游戏后，再开启识别"
      ],
      [
        367,
        306,
        234,
        33,
        "监听、记录内容和诊断按用途分区"
      ],
      [
        366,
        154,
        750,
        40,
        "敌对重开固定在分页上方；展开标题查看说明"
      ]
    ],
    "size": [
      1280,
      720
    ]
  },
  "mod": {
    "title": "Mod 加工：先准备加工器，再设置目标与功能",
    "file": "mod-processing.png",
    "crop": [
      355,
      111,
      773,
      588
    ],
    "note": "此图以原版游戏为来源，选择声纹识别与局内房间工具，输出名为 HubMain。此旧示例显示加工器缺失，因此开始按钮不可用；当前加工器随 Hub 内置，实际缺失时请修复 Hub 安装。示例没有执行加工。",
    "marks": [
      [
        1005,
        124,
        106,
        39,
        "旧版提示；当前加工器缺失时修复 Hub 安装"
      ],
      [
        376,
        300,
        282,
        36,
        "选择加工完成后要应用的目标账号"
      ],
      [
        376,
        590,
        282,
        68,
        "选择原版游戏或已有 Mod 作为来源"
      ],
      [
        691,
        253,
        418,
        113,
        "按需选择声纹识别与局内房间工具"
      ],
      [
        691,
        576,
        418,
        106,
        "填写新名称，准备完成后开始加工并应用"
      ]
    ]
  },
  "participants": {
    "title": "跟房准备：主号、小号、Mod 都要对应",
    "file": "room-participants.png",
    "crop": [
      240,
      229,
      764,
      515
    ],
    "note": "旧版示例：当前此页只选择账号及顺序，Mod 在账号或方案成员参数中配置；启动前按本次实际参数检查房间工具。所有参与者均需加载局内房间工具。",
    "marks": [
      [
        252,
        401,
        261,
        40,
        "指定唯一的主账号"
      ],
      [
        252,
        527,
        320,
        35,
        "勾选需要跟随的小号"
      ],
      [
        252,
        594,
        739,
        111,
        "旧版 Mod 状态区；当前在账号或方案参数中选用成品"
      ]
    ],
    "size": [
      1041,
      810
    ]
  },
  "method": {
    "title": "先手动跟随，再选择小号进房方式",
    "file": "room-method.png",
    "crop": [
      240,
      400,
      764,
      342
    ],
    "note": "当前使用后台同步预填，创建和加入快捷键可在任意 D2R 窗口触发，创建时自动切到配置主号。首次先用手动跟随验证，再按需要切换自动跟随。",
    "marks": [
      [
        252,
        585,
        82,
        36,
        "首次使用先选手动跟随"
      ],
      [
        628,
        585,
        168,
        36,
        "选择同时或间隔派发小号"
      ],
      [
        252,
        692,
        114,
        38,
        "设置主账号创建房间快捷键"
      ],
      [
        628,
        692,
        114,
        38,
        "设置跟随账号加入快捷键"
      ]
    ],
    "size": [
      1041,
      810
    ]
  },
  "paths": {
    "title": "只配置你实际使用的客户端",
    "file": "paths.png",
    "crop": [
      408,
      60,
      717,
      641
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
    "note": "旧版单独昵称步骤。当前向导分“账号信息、认证方式、连接账号”三步，第一步同时填写本地昵称、区服、界面语言和配音；昵称不是战网密码。",
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
      403,
      54,
      723,
      408
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
    "title": "按需添加扩展，从同一侧栏进入设置",
    "file": "modules.png",
    "crop": [
      143,
      64,
      991,
      541
    ],
    "note": "图中扩展均已添加，因此显示“设置”。首次使用先点“添加”；添加后可从左侧直接进入。识别与跟房还需要对应游戏 Mod，移除扩展会停止相关功能并保留配置。",
    "marks": [
      [
        1000,
        278,
        83,
        40,
        "识别与统计：添加后进入设置"
      ],
      [
        1000,
        379,
        83,
        40,
        "自动跟房：添加后进入设置"
      ],
      [
        149,
        304,
        176,
        142,
        "已添加扩展也可直接从左侧进入"
      ]
    ]
  },
  "rules": {
    "title": "房名由前缀、序号和位数一起决定",
    "file": "room-rules.png",
    "crop": [
      240,
      440,
      764,
      274
    ],
    "note": "示例 chaos- + 27 + 3 位得到 chaos-027。密码 pw 仅为示例，留空则创建无密码房间；修改会自动保存。",
    "marks": [
      [
        252,
        568,
        740,
        62,
        "填写房名开头、下一个序号和序号位数"
      ],
      [
        904,
        443,
        88,
        27,
        "核对下次房名预览"
      ],
      [
        252,
        643,
        262,
        57,
        "密码可选；留空就是无密码房间"
      ]
    ],
    "size": [
      1041,
      810
    ]
  },
  "auto": {
    "title": "切换自动跟随，并留足主号载入时间",
    "file": "room-auto.png",
    "crop": [
      240,
      400,
      764,
      342
    ],
    "note": "5 秒只是示例，应覆盖你机器上主号的实际载入时间。自动跟随仍由你触发主号建房；本图只展示配置，没有执行跟房任务。",
    "marks": [
      [
        338,
        585,
        82,
        36,
        "切到自动跟随"
      ],
      [
        252,
        692,
        114,
        38,
        "设置建房后等待秒数"
      ],
      [
        628,
        585,
        168,
        36,
        "选择小号进房方式"
      ],
      [
        628,
        692,
        114,
        38,
        "使用建房快捷键触发新房"
      ]
    ],
    "size": [
      1041,
      810
    ]
  }
};
const GUIDE_STEP_FIGURES = {
  paths: ['paths'], first: ['init'], second: ['init', 'accounts'],
  'audio-install': ['modules', 'recognition'], 'audio-mod': ['mod'], 'combined-mod': ['mod', 'accounts'],
  'room-install': ['modules', 'participants'], 'room-mod': ['participants', 'mod'],
  'room-method': ['method'], 'room-rules': ['rules', 'collaboration', 'method'], 'room-auto': ['auto']
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
      <image href="${base}${figure.file}" width="${figure.size?.[0] ?? 1280}" height="${figure.size?.[1] ?? 720}" />
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
