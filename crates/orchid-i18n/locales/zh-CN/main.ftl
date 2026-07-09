# Orchid Simplified Chinese (zh-CN) message catalog.
#
# Consumed by the upcoming `orchid-i18n::LocaleManager`. Until that lands,
# the built-in widgets fall back to the English strings baked into their
# Rust code — the keys below mirror those defaults and will become the
# single source of truth once the bundle loader is wired up.

# ---- Widget registry metadata ----
widget-terminal-name = 终端
widget-terminal-desc = 本地、WSL 或 SSH  shell，支持 PTY、ANSI 颜色和滚动缓冲

widget-weather-name = 天气
widget-weather-desc = 当前状况和 3 天预报

widget-moon-name = 月亮
widget-moon-desc = 当前月相、月出/月落时间和天体数据

widget-system-name = 系统
widget-system-desc = CPU、内存、磁盘、网络和电池指示器
# ---- Shared size / duration formatting ----
byte-size-b = { $value } B
byte-size-kb = { $value } KB
byte-size-mb = { $value } MB
byte-size-gb = { $value } GB
byte-size-tb = { $value } TB
duration-days-hours = { $days }天 { $hours }小时
duration-hours-minutes = { $hours }小时 { $minutes }分
duration-minutes = { $minutes }分
locale-name-ar-SA = العربية
locale-name-de-DE = Deutsch
locale-name-en-US = English (United States)
locale-name-es-ES = Español
locale-name-fr-FR = Français
locale-name-it-IT = Italiano
locale-name-ja-JP = 日本語
locale-name-ko-KR = 한국어
locale-name-pt-BR = Português (Brasil)
locale-name-ru-RU = Русский
locale-name-zh-CN = 简体中文

widget-rss-name = 新闻
widget-rss-desc = RSS 和 Atom 新闻源

widget-recent-files-name = 最近文件
widget-recent-files-desc = Orchid 中最近打开的文件

widget-search-name = 通用搜索
widget-search-desc = 搜索文件、运行命令、打开设置

widget-media-name = 媒体播放器
widget-media-desc = 正在播放及传输控制

widget-password-name = 密码
widget-password-desc = 访问您的密码数据库

widget-viewer-name = 查看器
widget-viewer-desc = 查看图片、文档、源文件和压缩包

# ---- Weather ----
weather-condition-clear = 晴朗
weather-condition-partly-cloudy = 局部多云
weather-condition-cloudy = 多云
weather-condition-overcast = 阴天
weather-condition-fog = 雾
weather-condition-drizzle = 毛毛雨
weather-condition-rain = 雨
weather-condition-snow = 雪
weather-condition-sleet = 雨夹雪
weather-condition-thunderstorm = 雷暴
weather-condition-hail = 冰雹
weather-condition-windy = 大风
weather-condition-unknown = 未知
weather-day-today = 今天
weather-day-tomorrow = 明天
weather-status-fresh = 最新
weather-status-stale = 数据可能已过期
weather-status-offline = 离线
weather-status-error = 加载天气出错
weather-updated-just-now = 刚刚更新
weather-updated-minutes = { $m } 分钟前更新
weather-updated-hours = { $h } 小时前更新
weather-updated-days = { $d } 天前更新

# ---- Relative time (shared) ----
relative-just-now = 刚刚
relative-minutes = { $m } 分钟前
relative-hours = { $h } 小时前
relative-days = { $d } 天前

weather-loading = 正在加载天气…
weather-feels-like = 体感 { $temp }
weather-humidity-label = 湿度
weather-wind-label = 风
weather-humidity-line = { $label } { $h }%
weather-wind-line = { $label } { $speed } km/h { $dir }
weather-wind-line-no-dir = { $label } { $speed } km/h

# ---- Wind directions ----
weather-wind-n = 北
weather-wind-nne = 北东北
weather-wind-ne = 东北
weather-wind-ene = 东东北
weather-wind-e = 东
weather-wind-ese = 东东南
weather-wind-se = 东南
weather-wind-sse = 南东南
weather-wind-s = 南
weather-wind-ssw = 南西南
weather-wind-sw = 西南
weather-wind-wsw = 西西南
weather-wind-w = 西
weather-wind-wnw = 西西北
weather-wind-nw = 西北
weather-wind-nnw = 北西北

# ---- Moon ----
moon-phase-new = 新月
moon-phase-waxing-crescent = 娥眉月
moon-phase-first-quarter = 上弦月
moon-phase-waxing-gibbous = 盈凸月
moon-phase-full = 满月
moon-phase-waning-gibbous = 亏凸月
moon-phase-last-quarter = 下弦月
moon-phase-waning-crescent = 残月
moon-illumination = 照亮 { $pct }%
moon-age = 月龄：{ $days } 天
moon-distance = 距离：{ $km } km
moon-next-full = 下次满月：{ $date }
moon-next-new = 下次新月：{ $date }
moon-moonrise = 月出：{ $time }
moon-moonset = 月落：{ $time }
moon-sunrise = 日出：{ $time }
moon-sunset = 日落：{ $time }
moon-libration = Libration：{ $lat }°，{ $lon }°
moon-loading = 正在计算月相数据…

# ---- System ----
system-cpu-label = CPU
system-memory-label = 内存
system-disk-label = 磁盘 { $mount }
system-network-label = 网络 { $name }
system-battery-label = 电池
system-uptime-label = 运行时间
system-battery-charging = 充电中
system-battery-time-remaining = 剩余 { $time }
system-network-rate = ↑ { $up }/s  ↓ { $down }/s
system-loading = 正在加载系统指标…

# ---- RSS ----
rss-no-feeds = 未配置订阅源
rss-loading = 正在加载新闻…
rss-fetch-failed = 无法加载订阅源。请检查连接后重试。
rss-empty = 已配置的订阅源中尚无条目。
recent-files-empty = 尚无最近文件。在查看器或文件管理器中打开文件即可在此显示。
recent-files-open-hint = 打开文件
rss-open-item-hint = 打开链接
rss-error-summary = { $total } 个订阅源中有 { $n } 个更新失败
rss-item-published-minutes = { $m } 分钟前
rss-item-published-hours = { $h } 小时前
rss-item-published-days = { $d } 天前

# ---- Universal Search ----
search-placeholder = 搜索文件、命令、设置…
search-empty-state = 开始输入以搜索
search-no-results = 没有“{ $query }”的结果
search-no-results-short = 无结果
search-sources-unconfigured = 搜索源尚未配置
search-searching = 搜索中…
search-source-files = 文件
search-source-commands = 命令
search-source-settings = 设置
command-terminal-invocation = orc { $verb }

# ---- Command palette ----
command-palette-placeholder = 运行命令…
command-palette-empty = 所有命令

# ---- Registered commands ----
command.widget.create.name = 创建小组件
command.widget.create.desc = 向工作区添加新小组件
command.widget.create.arg.type = 小组件类型 ID（如 terminal、weather）

command.widget.close.name = 关闭小组件
command.widget.close.desc = 关闭小组件实例

command.widget.move.name = 移动小组件
command.widget.resize.name = 调整小组件大小
command.widget.focus_next.name = 聚焦下一个小组件
command.widget.show_all.name = 显示所有小组件
command.widget.group.dissolve.name = 解散小组件组

command.workspace.create.name = 创建工作区
command.workspace.delete.name = 删除工作区
command.workspace.switch_to.name = 切换到工作区
command.workspace.switch_next.name = 下一个工作区
command.workspace.switch_previous.name = 上一个工作区

command.terminal.split_horizontal.name = 水平分割终端
command.terminal.split_vertical.name = 垂直分割终端
command.terminal.tab_new.name = 新建终端标签页
command.terminal.close.name = 关闭终端窗格或标签页
command.terminal.focus_next_pane.name = 聚焦下一个终端窗格
command.terminal.focus_previous_pane.name = 聚焦上一个终端窗格
command.terminal.tab_next.name = 下一个终端标签页
command.terminal.tab_previous.name = 上一个终端标签页

# ---- Settings (universal search) ----
settings.section.general = 常规
settings.section.appearance = 外观
settings.section.input = 输入
settings.section.shortcuts = 快捷键
settings.section.locale = 语言
settings.section.privacy = 隐私

# ---- Settings panel ----
settings-panel-title = 设置
settings-panel-hint = 值目前为只读。请直接编辑 config.toml；更改会自动重新加载。
settings-panel-coming-soon = 此部分的完整设置编辑器尚不可用。请暂时直接编辑 config.toml。
settings-panel-ok = 关闭

settings-open-in-editor = 在编辑器中打开
settings-open-config-file = 打开 config.toml
settings-value-none = 无
settings-value-leader-timeout = { $ms } 毫秒
settings-shortcut-binding = { $key } → { $cmd }
settings-shortcut-list-separator = 、
settings-value-default = 默认
settings-value-disabled = 已禁用
settings-value-system-default = 系统默认
settings-value-hand-left = 左
settings-value-hand-right = 右
settings-value-pen-double-tap-none = 无
settings-value-pen-double-tap-switch-tool = 切换工具
settings-value-pen-double-tap-erase = 擦除
settings-value-sunday = 星期日
settings-value-monday = 星期一

settings-field-auto-update = 自动更新
settings-field-telemetry = 遥测
settings-field-open-on-startup = 启动时打开
settings-field-theme = 主题
settings-field-density = 密度
settings-field-font-family = 字体
settings-field-font-scale = 字体缩放
settings-field-reduce-motion = 减少动画
settings-field-follow-system-theme = 跟随系统主题
settings-field-dark-theme = 深色主题
settings-field-light-theme = 浅色主题
settings-field-primary-hand = 惯用手
settings-field-mirror-edge-swipes = 镜像边缘滑动
settings-field-haptic-feedback = 触觉反馈
settings-field-palm-rejection = 防误触
settings-field-pen-double-tap = 笔双击
settings-field-shortcut-overrides = 快捷键覆盖
settings-field-leader-key = 引导键
settings-field-leader-timeout = 引导超时
settings-field-leader-bindings = 引导绑定
settings-field-language = 语言
settings-field-date-format = 日期格式
settings-field-time-format = 时间格式
settings-field-first-day-of-week = 每周第一天
settings-field-record-action-history = 记录操作历史
settings-field-history-retention-days = 历史保留（天）
settings-field-clear-clipboard-seconds = 复制后清除剪贴板

settings-field-vault-auto-lock = 保险库自动锁定（秒）
command.settings.open.name = 打开设置
command.settings.open.desc = 显示设置面板
command.settings.open_config_file.name = 打开配置
command.settings.open_config_file.desc = 在默认编辑器中打开 config.toml
command.password.lock.name = 锁定密码库
command.password.lock.desc = 从内存中清除已解锁的密码数据库

command.navigation.show_workspace_panel.name = 显示工作区面板
command.navigation.show_workspace_panel.desc = 切换工作区侧边栏
command.notification.show_center.name = 显示通知中心
command.notification.show_center.desc = 切换通知中心
command.dock.show.name = 显示程序坞
command.dock.show.desc = 切换小组件程序坞
command.search.show_universal.name = 全局搜索
command.search.show_universal.desc = 打开或聚焦全局搜索
command.onboarding.toggle_hint_mode.name = 切换提示模式
command.onboarding.toggle_hint_mode.desc = 显示或隐藏工作区上的手势提示
navigation-workspace-panel-title = 工作区
notification-center-title = 通知
notification-center-placeholder = 暂无通知。
notification-center-clear = 全部清除
notification-center-dismiss = 关闭
notification-center-tip-title = 提示
notification-center-tip-body = 从右边缘滑动或运行“显示通知中心”以打开此面板。
# ---- Terminal tab bar ----
terminal-tooltip-split-h = 水平分割 (Ctrl+Shift+H)
terminal-tooltip-split-v = 垂直分割 (Ctrl+Shift+J)
terminal-tooltip-tab-new = 新建标签页 (Ctrl+Shift+T)
terminal-tooltip-tab-close = 关闭标签页
terminal-tooltip-pane-close = 关闭窗格

# ---- Media player ----
media-no-session = 没有正在播放的媒体
media-loading = 正在加载媒体…
media-unsupported = 此平台不支持媒体控制
media-play = 播放
media-pause = 暂停
media-next = 下一首
media-previous = 上一首

# ---- Password manager ----
password-locked = 数据库已锁定
password-unlock-label = 主密码
password-unlock-placeholder = 输入主密码
password-unlock-submit = 解锁
password-unlock-biometric = Windows Hello
password-unlock-biometric-prompt = 解锁密码库
password-search-placeholder = 搜索条目…
password-no-entries = 尚无条目
password-copy-password = 复制密码
password-copy-username = 复制用户名
password-copy-totp = 复制 TOTP
password-open-url = 打开 URL
password-password-copied = 密码已复制（30 秒后清除）
password-totp-copied = TOTP 已复制（30 秒后清除）
password-totp-remaining = { $s } 秒


# ==== Viewer widget ====
widget-viewer-name = 查看器
widget-viewer-desc = 打开文件：图片、PDF、文本、压缩包
viewer-loading = 加载中…
viewer-error = 无法显示此文件
viewer-unsupported = 不支持的文件类型
viewer-image-fit-screen = 适应屏幕
viewer-image-actual-size = 实际大小
viewer-image-rotate = 旋转
viewer-image-flip-h = 水平翻转
viewer-image-flip-v = 垂直翻转
viewer-image-zoom-in = 放大
viewer-image-zoom-out = 缩小
viewer-image-rotate-cw = 顺时针旋转
viewer-image-rotate-ccw = 逆时针旋转
viewer-archive-root = (根目录)
viewer-archive-parent = 上级文件夹
viewer-pdf-page-of = 第 { $current } 页，共 { $total } 页
viewer-pdf-fit-width = 适应宽度
viewer-pdf-fit-page = 适应页面
viewer-pdf-go = 转到
viewer-pdf-prev-page = 上一页
viewer-pdf-next-page = 下一页
viewer-pdf-info = PDF · 第 { $current } / { $total } 页 · { $width } × { $height } px · { $zoom }%
viewer-image-info = { $width } × { $height } · { $size } · { $format }
viewer-image-format-avif = AVIF
viewer-image-format-bmp = BMP
viewer-image-format-gif = GIF
viewer-image-format-heic = HEIC
viewer-image-format-image = Image
viewer-image-format-jpeg = JPEG
viewer-image-format-png = PNG
viewer-image-format-raw = RAW
viewer-image-format-svg = SVG
viewer-image-format-tga = TGA
viewer-image-format-tiff = TIFF
viewer-image-format-webp = WebP
viewer-archive-info = { $format }，{ $count } 项
viewer-archive-format-7z = 7z
viewer-archive-format-tar = TAR
viewer-archive-format-tar-gz = TAR.GZ
viewer-archive-format-tar-xz = TAR.XZ
viewer-archive-format-zip = ZIP
viewer-archive-extracted-selected = 已提取到 { $path }
viewer-archive-extracted-all = 已将 { $count } 项提取到 { $path }
viewer-archive-nothing-selected = 未选择要提取的内容
viewer-archive-cannot-extract-folder = 无法提取文件夹
viewer-action-failed = 查看器操作失败：{ $reason }
viewer-text-save-failed = 无法保存文件：{ $reason }
viewer-text-read-only = 只读
viewer-text-editing = 编辑中
viewer-text-save = 保存 (Ctrl+S)
viewer-text-lines = { $count } 行
viewer-text-line-ending-lf = LF
viewer-text-line-ending-crlf = CRLF
viewer-encoding-big5 = Big5
viewer-encoding-euc-jp = EUC-JP
viewer-encoding-euc-kr = EUC-KR
viewer-encoding-gb18030 = GB18030
viewer-encoding-gbk = GBK
viewer-encoding-iso-8859-1 = ISO-8859-1
viewer-encoding-iso-8859-5 = ISO-8859-5
viewer-encoding-koi8-r = KOI8-R
viewer-encoding-shift-jis = Shift_JIS
viewer-encoding-utf-16be = UTF-16 BE
viewer-encoding-utf-16le = UTF-16 LE
viewer-encoding-utf-8 = UTF-8
viewer-encoding-windows-1251 = Windows-1251
viewer-encoding-windows-1252 = Windows-1252
viewer-syntax-bash = Shell
viewer-syntax-c = C
viewer-syntax-cpp = C++
viewer-syntax-csharp = C#
viewer-syntax-css = CSS
viewer-syntax-dockerfile = Dockerfile
viewer-syntax-go = Go
viewer-syntax-html = HTML
viewer-syntax-ini = INI
viewer-syntax-java = Java
viewer-syntax-javascript = JavaScript
viewer-syntax-json = JSON
viewer-syntax-kotlin = Kotlin
viewer-syntax-lua = Lua
viewer-syntax-markdown = Markdown
viewer-syntax-perl = Perl
viewer-syntax-php = PHP
viewer-syntax-plaintext = Plain text
viewer-syntax-python = Python
viewer-syntax-ruby = Ruby
viewer-syntax-rust = Rust
viewer-syntax-sql = SQL
viewer-syntax-swift = Swift
viewer-syntax-toml = TOML
viewer-syntax-typescript = TypeScript
viewer-syntax-xml = XML
viewer-syntax-yaml = YAML
viewer-text-unsaved-title = 未保存的更改
viewer-text-unsaved-body = 关闭前保存更改？
viewer-text-discard = 放弃

viewer-text-dirty-indicator = 未保存的更改
viewer-archive-extract-all = 全部解压
viewer-archive-extract-selected = 解压所选

# ==== File manager widget ====
widget-fm-name = 文件
widget-fm-desc = 浏览、整理和管理文件
fm-nav-back = 后退
fm-nav-forward = 前进
fm-nav-up = 上级
fm-nav-home = 主目录
fm-view-icons = 图标
fm-view-list = 列表
fm-view-details = 详细信息
fm-view-gallery = 画廊
fm-sort-name = 名称
fm-sort-size = 大小
fm-sort-modified = 修改时间
fm-sort-type = 类型
fm-action-open = 打开
fm-action-open-all = 全部打开
fm-action-open-with = 打开方式…
fm-action-open-default = 用默认应用打开
fm-action-open-in-viewer = 在 Orchid Viewer 中打开
fm-action-copy = 复制
fm-action-cut = 剪切
fm-action-paste = 粘贴
fm-action-rename = 重命名
fm-action-delete = 删除
fm-action-new-folder = 新建文件夹
fm-action-new-tab = 新建标签页
fm-action-close-tab = 关闭标签页
fm-action-select-all = 全选
fm-action-deselect-all = 取消全选
fm-action-star = 加星标
fm-action-unstar = 取消星标
fm-action-encrypt = 加密
fm-action-reveal = 临时显示
fm-action-decrypt = 解密
fm-action-add-tag = 添加标签…
fm-action-remove-tag = 移除标签
fm-action-color-label = 颜色标签
fm-color-red = 红色
fm-color-orange = 橙色
fm-color-yellow = 黄色
fm-color-green = 绿色
fm-color-blue = 蓝色
fm-color-purple = 紫色
fm-color-gray = 灰色
fm-color-none = 无颜色
fm-action-properties = 属性
fm-action-add-to-managed = 添加到托管文件夹
fm-action-remove-from-managed = 从托管文件夹移除
fm-action-managed-policy = 托管文件夹策略
fm-managed-policy-title = 托管文件夹策略
fm-policy-max-size = 最大大小
fm-policy-retention = 保留期限
fm-policy-excludes = 排除模式
fm-policy-unlimited = 无限制
fm-policy-forever = 永久保留
fm-policy-retention-days = { $days } 天
fm-policy-none = 无
fm-sidebar-managed-folder-policy = { $name }（{ $count } 个文件，节省 { $dedup }，策略）
fm-sidebar-managed-policy-only = { $name }（策略）
fm-rename-title = 重命名
fm-rename-ok = 确定
fm-rename-cancel = 取消
fm-dual-pane-on = 双窗格
fm-dual-pane-off = 单窗格
fm-show-hidden-on = 显示隐藏文件
fm-show-hidden-off = 隐藏隐藏文件
fm-click-single-on = 单击打开
fm-click-single-off = 双击打开
fm-encrypt-title = 使用密码短语加密
fm-reveal-title = 输入密码短语以显示
fm-decrypt-title = 输入密码短语以解密
fm-info-close = 关闭
fm-properties-title = 属性
fm-properties-kind-folder = 文件夹
fm-properties-kind-file = 文件
fm-properties-type = 类型：{ $kind }
fm-properties-size = 大小：{ $size }
fm-properties-modified = 修改时间：{ $modified }
fm-properties-mime = MIME：{ $mime }
fm-tag-add-title = 添加标签
fm-confirm-delete = 删除 { $n } 项？
fm-confirm-delete-permanent = 永久删除 { $n } 项？
fm-loading = 正在加载…
fm-status-bar = { $items } 项，已选 { $selected } 项
fm-status-managed = { $items } 项，已选 { $selected } 项 · { $tracked } 已摄取，{ $dedup } 已去重
fm-encrypted = 已加密：{ $name }
fm-decrypted = 已解密：{ $name }
fm-managed-added = 已添加到托管文件夹
fm-managed-removed = 已从托管文件夹移除
fm-encryption-unavailable = 加密不可用
fm-passphrase-failed = 密码短语失败：{ $reason }
fm-passphrase-invalid = 无效的密码短语
fm-passphrase-required = 需要密码短语
fm-decryption-failed = 解密失败
fm-passphrase-encrypt-hint = 请选择强密码短语。丢失后无法恢复。
fm-passphrase-decrypt-hint = 输入用于加密这些文件的密码短语。
fm-passphrase-reveal-hint = 文件将解密到临时位置以供查看。
fm-passphrase-biometric = Windows Hello
fm-passphrase-biometric-prompt = 解锁加密文件
fm-revealed = 已显示：{ $name }
fm-managed-unavailable = 托管文件夹不可用
fm-managed-no-selection = 选择要添加到托管文件夹的文件夹
fm-not-managed-folder = 不是托管文件夹
fm-managed-conflict = 托管文件夹冲突
fm-sidebar-managed-folder = { $name }（{ $count } 个文件，节省 { $dedup }）
fm-ingest-failed = 摄取失败：{ $name }
fm-quick-filter-placeholder = 筛选…
fm-sidebar-favorites = 收藏夹
fm-sidebar-categories = 类别
fm-sidebar-managed = 托管文件夹
fm-network-placeholder = 未配置网络挂载。请在 config.toml 中添加 [[file-manager.network-mounts]] 条目（通过 rclone 支持 SFTP、SMB、WebDAV、FTP）。
fm-network-no-provider = 此网络位置未注册文件系统提供程序。
fm-network-rclone-missing = rclone 未安装或不在 PATH 中。如需要请设置 RCLONE_BIN。
fm-network-invalid-mount = 此网络挂载配置错误。请检查 config.toml 中的名称和 URI。
fm-network-auth-failed = 身份验证失败。请检查 config.toml 中的用户名和密码。
fm-network-permission-denied = 此网络位置权限被拒绝。
fm-network-connection-failed = 无法连接到网络主机。请检查 URI 和网络。
fm-ingested = 已摄取：{ $name }
fm-ingesting = 正在摄取：{ $name }（{ $count } 个活动）
fm-ingesting-count = 正在摄取 { $count } 个文件…
fm-copying = 正在复制：{ $name }（{ $percent }%）
fm-moving = 正在移动：{ $name }（{ $percent }%）
fm-transfer-failed = 传输失败：{ $reason }
fm-action-failed = 文件操作失败：{ $reason }
fm-invalid-folder-name = 文件夹名称无效
fm-no-provider-parent = 无法访问父文件夹
fm-no-parent-folder = 没有父文件夹
fm-selection-multiple-folders = 所选内容跨越多个文件夹
fm-invalid-rename-target = 重命名目标无效
fm-cannot-rename-root = 无法重命名根目录
fm-no-provider-path = 无法访问此路径
fm-empty-tag = 标签名称不能为空
fm-drop-not-directory = 放置目标不是文件夹
fm-drop-unavailable = 放置目标不可用
fm-type-ext-file = { $ext } 文件
fm-transfer-already-exists = 已存在同名文件
fm-transfer-virtual-dest = 无法复制或移动到虚拟文件夹
fm-clipboard-copy = { $count } 个条目可供粘贴
fm-clipboard-cut = { $count } 个条目（剪切）可供粘贴
fm-sidebar-network = 网络
fm-sidebar-network-all = 所有位置
fm-category-images = 图片
fm-category-documents = 文档
fm-category-video = 视频
fm-category-audio = 音频
fm-category-archives = 压缩包
fm-virtual-recent = 最近
fm-virtual-starred = 已加星标
fm-virtual-tags = 标签
fm-virtual-recent-empty = 尚无最近文件。打开文件即可在此显示。
fm-virtual-starred-empty = 尚无星标文件。从上下文菜单添加星标。
fm-virtual-tags-empty = 尚无带标签的文件。从上下文菜单添加标签。
fm-virtual-category-empty = 此类别中未找到匹配的文件。
fm-virtual-create-denied = 无法在虚拟位置创建文件夹
fm-empty-folder = 此文件夹为空
fm-entry-encrypted-hint = Encrypted file
fm-entry-managed-hint = Managed folder
fm-error-access = 无法访问此位置


# ==== Startup shell (task 11A) ====
window-title = Orchid
startup-welcome = 欢迎使用 Orchid
startup-subtitle = 触控优先的计算环境
startup-version-label = 版本 { $version }
status-theme = 主题：
status-language = 语言：
status-density = 密度：
density-touch = 触控
density-mouse = 鼠标
density-hybrid = 混合

# ---- Workspace shell (task 11B) ----
startup-get-started = 开始使用
onboarding-back = 返回
onboarding-next = 下一步
onboarding-skip = 跳过导览
onboarding-finish = 开始使用
onboarding-step-progress = 第 { $current } 步，共 { $total } 步
onboarding-step-welcome-title = 欢迎使用 Orchid
onboarding-step-welcome-body = Orchid 是一个触控优先的工作区，手势、命令和小组件是同一操作的三种形式。本简短导览将介绍基本功能。
onboarding-step-workspace-title = 您的工作区
onboarding-step-workspace-body = 在顶部切换工作区，在画布上排列小组件，并从底部程序坞添加新小组件。
onboarding-step-palette-title = 命令面板
onboarding-step-palette-body = 按 Ctrl+Shift+P 运行任意命令。每项都显示键盘快捷键，方便您边用边学。
onboarding-step-gestures-title = 手势与提示
onboarding-step-gestures-body = 从屏幕边缘滑动打开面板和程序坞。随时按 Win+? 切换提示模式，查看当前可用的操作。
onboarding-hint-workspace = 从左边缘滑动打开工作区
onboarding-hint-dock = 从底边缘向上滑动打开程序坞
onboarding-hint-gestures = Win+? 切换这些提示
workspace-default-name = 主工作区
workspace-new = 新建工作区
workspace-placement-blocked-title = 无法在此放置小组件
workspace-placement-blocked-body = 该位置与其他小组件重叠或超出网格。请选择空闲单元格。
group-tooltip-dissolve = 取消小组件分组
group-tooltip-move-left = 向左移动标签
group-tooltip-move-right = 向右移动标签
group-tooltip-close-tab = 从组中移除
group-hint-alt-detach = Alt+拖动以从组中分离
workspace-unnamed = 工作区 { $n }
dock-add-label = 添加小组件
catalog-title = 小组件目录
catalog-search-placeholder = 搜索小组件…
dock-widget-terminal = 终端
dock-widget-weather = 天气
dock-widget-moon = 月亮
dock-widget-system = 系统
dock-widget-rss = 新闻
dock-widget-recent-files = 最近
dock-widget-search = 搜索
dock-widget-media = 媒体
dock-widget-password = 密码
dock-widget-viewer = 查看器
dock-widget-fm = 文件

viewer-no-file = 未打开文件
viewer-loading-path = 正在加载：{ $path }
viewer-error-with-reason = 无法显示此文件：{ $reason }
viewer-error-archive-entry-not-found = Archive entry not found
viewer-error-file-too-large = This file is too large to open
viewer-error-image-decode = Could not decode this image
viewer-error-parse-text = Could not read this text file
viewer-error-pdf-empty = This PDF has no pages
viewer-error-pdf-render = Could not render this PDF page
viewer-error-syntax-grammar = Syntax highlighting is unavailable for this language
viewer-error-thumbnail = Could not generate a thumbnail
viewer-pdf-unavailable = 此版本不支持 PDF。
viewer-image-heic-unsupported = 暂不支持 HEIC 图片
viewer-image-raw-unsupported = 暂不支持 RAW 图片
viewer-archive-select-preview = 选择要预览的文件
viewer-archive-binary-preview = 二进制文件，{ $size }

password-select-entry = 选择条目
password-label-title = 标题
password-label-username = 用户名
password-label-password = 密码
password-label-url = URL
password-label-notes = 备注
password-label-totp = TOTP
password-action-lock = 锁定
password-action-add = 添加
password-add-title = 新建条目
password-add-submit = 保存
password-add-cancel = 取消
password-generate = 生成
password-add-error-title = 标题为必填项
password-add-error-duplicate = An entry with this title already exists
password-error-biometric-cancelled = Biometric unlock was cancelled
password-error-biometric-failed = Biometric unlock failed
password-error-biometric-unavailable = Biometric unlock is unavailable
password-error-db-open = Could not open the password database
password-error-entry-not-found = Entry not found
password-error-invalid-master = Incorrect master password
password-error-no-master-key = No biometric key is stored for this vault
password-error-unavailable = Password vault is unavailable
password-error-vault-locked = Vault is locked
password-error-with-reason = Password vault error: { $reason }
password-entry-added = 条目已保存

password-username-copied = 用户名已复制

moon-age-label = 月龄
moon-distance-label = 距离
moon-next-full-label = 下次满月
moon-next-new-label = 下次新月
moon-moonrise-label = 月出
moon-moonset-label = 月落
moon-sunrise-label = 日出
moon-sunset-label = 日落
moon-libration-label = Libration

widget-title-terminal = 终端
widget-close-tooltip = 关闭小组件
widget-close-confirm = 关闭 { $name }？
action-confirm-yes = 是
action-confirm-no = 否

fm-confirm-title = 确认
