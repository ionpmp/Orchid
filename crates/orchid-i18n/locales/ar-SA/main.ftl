# Orchid Arabic (ar-SA) message catalog.
#
# Consumed by the upcoming `orchid-i18n::LocaleManager`. Until that lands,
# the built-in widgets fall back to the English strings baked into their
# Rust code — the keys below mirror those defaults and will become the
# single source of truth once the bundle loader is wired up.

# ---- Widget registry metadata ----
widget-terminal-name = الطرفية
widget-terminal-desc = أصداف محلية أو WSL أو SSH مع PTY وألوان ANSI وسجل تمرير

widget-weather-name = الطقس
widget-weather-desc = الأحوال الحالية وتوقعات 3 أيام

widget-moon-name = القمر
widget-moon-desc = طور القمر الحالي وأوقات الشروق/الغروب والبيانات الفلكية

widget-system-name = النظام
widget-system-desc = مؤشرات وحدة المعالجة والذاكرة والقرص والشبكة والبطارية
# ---- Shared size / duration formatting ----
byte-size-b = { $value } بايت
byte-size-kb = { $value } ك.ب
byte-size-mb = { $value } م.ب
byte-size-gb = { $value } ج.ب
byte-size-tb = { $value } ت.ب
duration-days-hours = { $days }ي { $hours }س
duration-hours-minutes = { $hours }س { $minutes }د
duration-minutes = { $minutes }د
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

widget-rss-name = الأخبار
widget-rss-desc = خلاصات أخبار RSS وAtom

widget-recent-files-name = الملفات الأخيرة
widget-recent-files-desc = الملفات المفتوحة مؤخرًا في Orchid

widget-search-name = البحث الشامل
widget-search-desc = البحث في الملفات وتشغيل الأوامر وفتح الإعدادات

widget-media-name = مشغّل الوسائط
widget-media-desc = التشغيل الحالي مع عناصر التحكم

widget-password-name = كلمات المرور
widget-password-desc = الوصول إلى قاعدة بيانات كلمات المرور

widget-viewer-name = العارض
widget-viewer-desc = عرض الصور والمستندات وملفات المصدر والأرشيفات

# ---- Weather ----
weather-condition-clear = صافٍ
weather-condition-partly-cloudy = غائم جزئيًا
weather-condition-cloudy = غائم
weather-condition-overcast = ملبد بالغيوم
weather-condition-fog = ضباب
weather-condition-drizzle = رذاذ
weather-condition-rain = مطر
weather-condition-snow = ثلج
weather-condition-sleet = مطر ثلجي
weather-condition-thunderstorm = عاصفة رعدية
weather-condition-hail = برد
weather-condition-windy = عاصف
weather-condition-unknown = غير معروف
weather-day-today = اليوم
weather-day-tomorrow = غدًا
weather-status-fresh = محدّث
weather-status-stale = قد تكون البيانات قديمة
weather-status-offline = غير متصل
weather-status-error = خطأ في تحميل الطقس
weather-updated-just-now = تم التحديث الآن
weather-updated-minutes = تم التحديث منذ { $m } د
weather-updated-hours = تم التحديث منذ { $h } س
weather-updated-days = تم التحديث منذ { $d } ي

# ---- Relative time (shared) ----
relative-just-now = الآن
relative-minutes = منذ { $m } د
relative-hours = منذ { $h } س
relative-days = منذ { $d } ي

weather-loading = جارٍ تحميل الطقس…
weather-feels-like = يشعر كـ { $temp }
weather-humidity-label = الرطوبة
weather-wind-label = الرياح
weather-humidity-line = { $label } { $h }%
weather-wind-line = { $label } { $speed } km/h { $dir }
weather-wind-line-no-dir = { $label } { $speed } km/h

# ---- Wind directions ----
weather-wind-n = ش
weather-wind-nne = ش ش ق
weather-wind-ne = ش ق
weather-wind-ene = ق ش ق
weather-wind-e = ق
weather-wind-ese = ق ج ق
weather-wind-se = ج ق
weather-wind-sse = ج ج ق
weather-wind-s = ج
weather-wind-ssw = ج ج غ
weather-wind-sw = ج غ
weather-wind-wsw = غ ج غ
weather-wind-w = غ
weather-wind-wnw = غ ش غ
weather-wind-nw = ش غ
weather-wind-nnw = ش ش غ

# ---- Moon ----
moon-phase-new = محاق
moon-phase-waxing-crescent = هلال متزايد
moon-phase-first-quarter = تربيع أول
moon-phase-waxing-gibbous = أحدب متزايد
moon-phase-full = بدر
moon-phase-waning-gibbous = أحدب متناقص
moon-phase-last-quarter = تربيع أخير
moon-phase-waning-crescent = هلال متناقص
moon-illumination = { $pct }% مضاء
moon-age = العمر: { $days } أيام
moon-distance = المسافة: { $km } كم
moon-next-full = البدر التالي: { $date }
moon-next-new = المحاق التالي: { $date }
moon-moonrise = شروق القمر: { $time }
moon-moonset = غروب القمر: { $time }
moon-sunrise = شروق الشمس: { $time }
moon-sunset = غروب الشمس: { $time }
moon-libration = Libration: { $lat }°، { $lon }°
moon-loading = جارٍ حساب بيانات القمر…

# ---- System ----
system-cpu-label = CPU
system-memory-label = الذاكرة
system-disk-label = القرص { $mount }
system-network-label = الشبكة { $name }
system-battery-label = البطارية
system-uptime-label = وقت التشغيل
system-battery-charging = قيد الشحن
system-battery-time-remaining = متبقٍ { $time }
system-network-rate = ↑ { $up }/ث  ↓ { $down }/ث
system-loading = جارٍ تحميل مقاييس النظام…

# ---- RSS ----
rss-no-feeds = لا توجد خلاصات مُعدّة
rss-loading = جارٍ تحميل الأخبار…
rss-fetch-failed = تعذّر تحميل الخلاصات. تحقق من الاتصال وحاول مرة أخرى.
rss-empty = لا توجد عناصر في الخلاصات المُعدّة بعد.
recent-files-empty = لا توجد ملفات حديثة بعد. افتح ملفات في العارض أو مدير الملفات لعرضها هنا.
recent-files-open-hint = فتح الملف
rss-open-item-hint = فتح الرابط
rss-error-summary = فشل تحديث { $n } من { $total } خلاصات
rss-item-published-minutes = منذ { $m } د
rss-item-published-hours = منذ { $h } س
rss-item-published-days = منذ { $d } ي

# ---- Universal Search ----
search-placeholder = اكتب للبحث في الملفات والأوامر والإعدادات…
search-empty-state = ابدأ الكتابة للبحث
search-no-results = لا نتائج لـ «{ $query }»
search-no-results-short = لا نتائج
search-sources-unconfigured = مصادر البحث غير مُعدّة بعد
search-searching = جارٍ البحث…
search-source-files = الملفات
search-source-commands = الأوامر
search-source-settings = الإعدادات
command-terminal-invocation = orc { $verb }

# ---- Command palette ----
command-palette-placeholder = تشغيل أمر…
command-palette-empty = جميع الأوامر

# ---- Registered commands ----
command.widget.create.name = إنشاء عنصر واجهة
command.widget.create.desc = إضافة عنصر واجهة جديد إلى مساحة العمل
command.widget.create.arg.type = معرّف نوع عنصر الواجهة (مثل terminal، weather)

command.widget.close.name = إغلاق عنصر الواجهة
command.widget.close.desc = إغلاق مثيل عنصر الواجهة

command.widget.move.name = نقل عنصر الواجهة
command.widget.resize.name = تغيير حجم عنصر الواجهة
command.widget.focus_next.name = التركيز على عنصر الواجهة التالي
command.widget.show_all.name = إظهار جميع عناصر الواجهة
command.widget.group.dissolve.name = حل مجموعة عناصر الواجهة

command.workspace.create.name = إنشاء مساحة عمل
command.workspace.delete.name = حذف مساحة العمل
command.workspace.switch_to.name = التبديل إلى مساحة العمل
command.workspace.switch_next.name = مساحة العمل التالية
command.workspace.switch_previous.name = مساحة العمل السابقة

command.terminal.split_horizontal.name = تقسيم الطرفية أفقيًا
command.terminal.split_vertical.name = تقسيم الطرفية عموديًا
command.terminal.tab_new.name = علامة تبويب طرفية جديدة
command.terminal.close.name = إغلاق جزء أو علامة تبويب الطرفية
command.terminal.focus_next_pane.name = التركيز على جزء الطرفية التالي
command.terminal.focus_previous_pane.name = التركيز على جزء الطرفية السابق
command.terminal.tab_next.name = علامة تبويب الطرفية التالية
command.terminal.tab_previous.name = علامة تبويب الطرفية السابقة

# ---- Settings (universal search) ----
settings.section.general = عام
settings.section.appearance = المظهر
settings.section.input = الإدخال
settings.section.shortcuts = الاختصارات
settings.section.locale = اللغة
settings.section.privacy = الخصوصية

# ---- Settings panel ----
settings-panel-title = الإعدادات
settings-panel-hint = القيم للقراءة فقط حاليًا. عدّل config.toml مباشرة؛ تُعاد تحميل التغييرات تلقائيًا.
settings-panel-coming-soon = محرّر الإعدادات الكامل لهذا القسم غير متاح بعد. عدّل config.toml مباشرة في الوقت الحالي.
settings-panel-ok = إغلاق

settings-open-in-editor = فتح في المحرّر
settings-open-config-file = فتح config.toml
settings-value-none = لا شيء
settings-value-leader-timeout = { $ms } مللي ثانية
settings-shortcut-binding = { $key } → { $cmd }
settings-shortcut-list-separator = ، 
settings-value-default = افتراضي
settings-value-disabled = معطّل
settings-value-system-default = افتراضي النظام
settings-value-hand-left = يسار
settings-value-hand-right = يمين
settings-value-pen-double-tap-none = لا شيء
settings-value-pen-double-tap-switch-tool = تبديل الأداة
settings-value-pen-double-tap-erase = مسح
settings-value-sunday = الأحد
settings-value-monday = الاثنين

settings-field-auto-update = التحديث التلقائي
settings-field-telemetry = القياس عن بُعد
settings-field-open-on-startup = الفتح عند بدء التشغيل
settings-field-theme = السمة
settings-field-density = الكثافة
settings-field-font-family = عائلة الخط
settings-field-font-scale = مقياس الخط
settings-field-reduce-motion = تقليل الحركة
settings-field-follow-system-theme = اتباع سمة النظام
settings-field-dark-theme = السمة الداكنة
settings-field-light-theme = السمة الفاتحة
settings-field-primary-hand = اليد الأساسية
settings-field-mirror-edge-swipes = عكس تمرير الحواف
settings-field-haptic-feedback = ردود فعل لمسية
settings-field-palm-rejection = رفض راحة اليد
settings-field-pen-double-tap = النقر المزدوج للقلم
settings-field-shortcut-overrides = تجاوزات الاختصارات
settings-field-leader-key = مفتاح القائد
settings-field-leader-timeout = مهلة القائد
settings-field-leader-bindings = ارتباطات القائد
settings-field-language = اللغة
settings-field-date-format = تنسيق التاريخ
settings-field-time-format = تنسيق الوقت
settings-field-first-day-of-week = أول يوم في الأسبوع
settings-field-record-action-history = تسجيل سجل الإجراءات
settings-field-history-retention-days = الاحتفاظ بالسجل (أيام)
settings-field-clear-clipboard-seconds = مسح الحافظة بعد النسخ
settings-field-vault-auto-lock = قفل الخزنة تلقائياً (ثوانٍ)

command.settings.open.name = فتح الإعدادات
command.settings.open.desc = إظهار لوحة الإعدادات
command.settings.open_config_file.name = فتح الإعدادات
command.settings.open_config_file.desc = فتح config.toml في المحرّر الافتراضي
command.password.lock.name = قفل خزنة كلمات المرور
command.password.lock.desc = مسح قاعدة بيانات كلمات المرور غير المقفلة من الذاكرة

command.navigation.show_workspace_panel.name = إظهار لوحة مساحات العمل
command.navigation.show_workspace_panel.desc = إظهار أو إخفاء الشريط الجانبي لمساحات العمل
command.notification.show_center.name = إظهار مركز الإشعارات
command.notification.show_center.desc = إظهار أو إخفاء مركز الإشعارات
command.dock.show.name = إظهار الشريط السفلي
command.dock.show.desc = إظهار أو إخفاء شريط عناصر الواجهة
command.search.show_universal.name = البحث الشامل
command.search.show_universal.desc = فتح البحث الشامل أو التركيز عليه
command.onboarding.toggle_hint_mode.name = تبديل وضع التلميحات
command.onboarding.toggle_hint_mode.desc = إظهار أو إخفاء تلميحات الإيماءات في مساحة العمل
navigation-workspace-panel-title = مساحات العمل
notification-center-title = الإشعارات
notification-center-placeholder = لا توجد إشعارات بعد.
notification-center-clear = مسح الكل
notification-center-dismiss = تجاهل
notification-center-tip-title = تلميح
notification-center-tip-body = اسحب من الحافة اليسرى أو نفّذ «إظهار مركز الإشعارات» لفتح هذه اللوحة.

# ---- Terminal tab bar ----
terminal-tooltip-split-h = تقسيم أفقي (Ctrl+Shift+H)
terminal-tooltip-split-v = تقسيم عمودي (Ctrl+Shift+J)
terminal-tooltip-tab-new = علامة تبويب جديدة (Ctrl+Shift+T)
terminal-tooltip-tab-close = إغلاق علامة التبويب
terminal-tooltip-pane-close = إغلاق اللوحة

# ---- Media player ----
media-no-session = لا يوجد تشغيل
media-loading = جارٍ تحميل الوسائط…
media-unsupported = عناصر التحكم بالوسائط غير متاحة على هذه المنصة
media-play = تشغيل
media-pause = إيقاف مؤقت
media-next = التالي
media-previous = السابق

# ---- Password manager ----
password-locked = قاعدة البيانات مقفلة
password-unlock-label = كلمة المرور الرئيسية
password-unlock-placeholder = أدخل كلمة المرور الرئيسية
password-unlock-submit = إلغاء القفل
password-unlock-biometric = Windows Hello
password-unlock-biometric-prompt = إلغاء قفل خزنة كلمات المرور
password-search-placeholder = البحث في الإدخالات…
password-no-entries = لا توجد إدخالات بعد
password-copy-password = نسخ كلمة المرور
password-copy-username = نسخ اسم المستخدم
password-copy-totp = نسخ TOTP
password-open-url = فتح URL
password-password-copied = تم نسخ كلمة المرور (تُمسح خلال 30 ث)
password-totp-copied = تم نسخ TOTP (يُمسح خلال 30 ث)
password-totp-remaining = { $s } ث


# ==== Viewer widget ====
widget-viewer-name = العارض
widget-viewer-desc = فتح الملفات: صور، PDF، نص، أرشيفات
viewer-loading = جارٍ التحميل…
viewer-error = تعذّر عرض هذا الملف
viewer-unsupported = نوع ملف غير مدعوم
viewer-image-fit-screen = ملاءمة الشاشة
viewer-image-actual-size = الحجم الفعلي
viewer-image-rotate = تدوير
viewer-image-flip-h = قلب أفقي
viewer-image-flip-v = قلب عمودي
viewer-image-zoom-in = تكبير
viewer-image-zoom-out = تصغير
viewer-image-rotate-cw = تدوير باتجاه عقارب الساعة
viewer-image-rotate-ccw = تدوير عكس عقارب الساعة
viewer-archive-root = (الجذر)
viewer-archive-parent = المجلد الأب
viewer-pdf-page-of = صفحة { $current } من { $total }
viewer-pdf-fit-width = ملاءمة العرض
viewer-pdf-fit-page = ملاءمة الصفحة
viewer-pdf-go = انتقال
viewer-pdf-prev-page = الصفحة السابقة
viewer-pdf-next-page = الصفحة التالية
viewer-pdf-info = PDF · صفحة { $current } / { $total } · { $width } × { $height } px · { $zoom }%
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
viewer-archive-info = { $format }، { $count } إدخالات
viewer-archive-format-7z = 7z
viewer-archive-format-tar = TAR
viewer-archive-format-tar-gz = TAR.GZ
viewer-archive-format-tar-xz = TAR.XZ
viewer-archive-format-zip = ZIP
viewer-archive-extracted-selected = تم الاستخراج إلى { $path }
viewer-archive-extracted-all = تم استخراج { $count } إدخالات إلى { $path }
viewer-archive-nothing-selected = لم يتم تحديد أي شيء للاستخراج
viewer-archive-cannot-extract-folder = لا يمكن استخراج مجلد
viewer-action-failed = فشلت عملية العارض: { $reason }
viewer-text-save-failed = تعذر حفظ الملف: { $reason }
viewer-text-read-only = للقراءة فقط
viewer-text-editing = تحرير
viewer-text-save = حفظ (Ctrl+S)
viewer-text-lines = { $count } أسطر
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
viewer-text-unsaved-title = تغييرات غير محفوظة
viewer-text-unsaved-body = هل تريد حفظ التغييرات قبل الإغلاق؟
viewer-text-discard = تجاهل

viewer-text-dirty-indicator = تغييرات غير محفوظة
viewer-archive-extract-all = استخراج الكل
viewer-archive-extract-selected = استخراج المحدد

# ==== File manager widget ====
widget-fm-name = الملفات
widget-fm-desc = تصفح وتنظيم وإدارة الملفات
fm-nav-back = رجوع
fm-nav-forward = تقدّم
fm-nav-up = أعلى
fm-nav-home = الرئيسية
fm-view-icons = أيقونات
fm-view-list = قائمة
fm-view-details = تفاصيل
fm-view-gallery = معرض
fm-sort-name = الاسم
fm-sort-size = الحجم
fm-sort-modified = التعديل
fm-sort-type = النوع
fm-action-open = فتح
fm-action-open-all = فتح الكل
fm-action-open-with = فتح باستخدام…
fm-action-open-default = فتح بالتطبيق الافتراضي
fm-action-open-in-viewer = فتح في Orchid Viewer
fm-action-copy = نسخ
fm-action-cut = قص
fm-action-paste = لصق
fm-action-rename = إعادة تسمية
fm-action-delete = حذف
fm-action-new-folder = مجلد جديد
fm-action-new-tab = علامة تبويب جديدة
fm-action-close-tab = إغلاق علامة التبويب
fm-action-select-all = تحديد الكل
fm-action-deselect-all = إلغاء تحديد الكل
fm-action-star = تمييز بنجمة
fm-action-unstar = إزالة النجمة
fm-action-encrypt = تشفير
fm-action-reveal = إظهار مؤقتًا
fm-action-decrypt = فك التشفير
fm-action-add-tag = إضافة وسم…
fm-action-remove-tag = إزالة الوسم
fm-action-color-label = تصنيف لوني
fm-color-red = أحمر
fm-color-orange = برتقالي
fm-color-yellow = أصفر
fm-color-green = أخضر
fm-color-blue = أزرق
fm-color-purple = بنفسجي
fm-color-gray = رمادي
fm-color-none = بلا لون
fm-action-properties = خصائص
fm-action-add-to-managed = إضافة إلى مجلد مُدار
fm-action-remove-from-managed = إزالة من المجلدات المُدارة
fm-action-managed-policy = سياسة المجلد المُدار
fm-managed-policy-title = سياسة المجلد المُدار
fm-policy-max-size = الحجم الأقصى
fm-policy-retention = الاحتفاظ
fm-policy-excludes = أنماط الاستثناء
fm-policy-unlimited = بلا حد
fm-policy-forever = الاحتفاظ دائمًا
fm-policy-retention-days = { $days } يومًا
fm-policy-none = لا شيء
fm-sidebar-managed-folder-policy = { $name } ({ $count } ملفات، { $dedup } موفَّرة، سياسة)
fm-sidebar-managed-policy-only = { $name } (سياسة)
fm-rename-title = إعادة تسمية
fm-rename-ok = موافق
fm-rename-cancel = إلغاء
fm-dual-pane-on = جزء مزدوج
fm-dual-pane-off = جزء واحد
fm-show-hidden-on = إظهار الملفات المخفية
fm-show-hidden-off = إخفاء الملفات المخفية
fm-click-single-on = نقرة واحدة للفتح
fm-click-single-off = نقرتان للفتح
fm-encrypt-title = تشفير بعبارة مرور
fm-reveal-title = أدخل عبارة المرور للإظهار
fm-decrypt-title = أدخل عبارة المرور لفك التشفير
fm-info-close = إغلاق
fm-properties-title = خصائص
fm-properties-kind-folder = مجلد
fm-properties-kind-file = ملف
fm-properties-type = النوع: { $kind }
fm-properties-size = الحجم: { $size }
fm-properties-modified = معدّل: { $modified }
fm-properties-mime = MIME: { $mime }
fm-tag-add-title = إضافة وسم
fm-confirm-delete = حذف { $n } عناصر؟
fm-confirm-delete-permanent = حذف { $n } عناصر نهائيًا؟
fm-loading = جارٍ التحميل…
fm-status-bar = { $items } عناصر، { $selected } محدد
fm-status-managed = { $items } عناصر، { $selected } محدد · { $tracked } مُستَوعَب، { $dedup } مُزال التكرار
fm-encrypted = مُشفّر: { $name }
fm-decrypted = مُفكوك التشفير: { $name }
fm-managed-added = أُضيف إلى مجلد مُدار
fm-managed-removed = أُزيل من المجلدات المُدارة
fm-encryption-unavailable = التشفير غير متاح
fm-passphrase-failed = فشلت عبارة المرور: { $reason }
fm-passphrase-invalid = عبارة مرور غير صالحة
fm-passphrase-required = عبارة المرور مطلوبة
fm-decryption-failed = فشل فك التشفير
fm-passphrase-encrypt-hint = اختر عبارة مرور قوية. لا يمكن استردادها عند فقدانها.
fm-passphrase-decrypt-hint = أدخل عبارة المرور المستخدمة لتشفير هذه الملفات.
fm-passphrase-reveal-hint = تُفكّ الملفات إلى موقع مؤقت للعرض.
fm-passphrase-biometric = Windows Hello
fm-passphrase-biometric-prompt = إلغاء قفل الملفات المشفّرة
fm-revealed = مُظهَر: { $name }
fm-managed-unavailable = المجلدات المُدارة غير متاحة
fm-managed-no-selection = حدّد مجلدًا لإضافته إلى المجلدات المُدارة
fm-not-managed-folder = ليس مجلدًا مُدارًا
fm-managed-conflict = تعارض مجلد مُدار
fm-sidebar-managed-folder = { $name } ({ $count } ملفات، { $dedup } موفّر)
fm-ingest-failed = فشل الاستيعاب: { $name }
fm-quick-filter-placeholder = تصفية…
fm-sidebar-favorites = المفضلة
fm-sidebar-categories = الفئات
fm-sidebar-managed = المجلدات المُدارة
fm-network-placeholder = لم يتم تكوين ربطات الشبكة. أضف إدخالات [[file-manager.network-mounts]] في config.toml (SFTP وSMB وWebDAV وFTP عبر rclone).
fm-network-no-provider = لا يوجد موفر نظام ملفات مسجّل لهذا الموقع الشبكي.
fm-network-rclone-missing = rclone غير مثبت أو غير موجود في PATH. عيّن RCLONE_BIN إذا لزم الأمر.
fm-network-invalid-mount = mount الشبكة هذا مُعدّ بشكل خاطئ. تحقق من الاسم وURI في config.toml.
fm-network-auth-failed = فشلت المصادقة. تحقق من اسم المستخدم وكلمة المرور في config.toml.
fm-network-permission-denied = تم رفض الإذن في هذا الموقع الشبكي.
fm-network-connection-failed = تعذّر الاتصال بمضيف الشبكة. تحقق من URI والشبكة.
fm-ingested = مُستَوعَب: { $name }
fm-ingesting = استيعاب: { $name } ({ $count } نشط)
fm-ingesting-count = جارٍ استيعاب { $count } ملفات…
fm-copying = نسخ: { $name } ({ $percent }%)
fm-moving = نقل: { $name } ({ $percent }%)
fm-transfer-failed = فشل النقل: { $reason }
fm-action-failed = فشلت عملية الملف: { $reason }
fm-invalid-folder-name = اسم مجلد غير صالح
fm-no-provider-parent = تعذر الوصول إلى المجلد الأصل
fm-no-parent-folder = لا يوجد مجلد أصل
fm-selection-multiple-folders = التحديد يمتد عبر مجلدات متعددة
fm-invalid-rename-target = هدف إعادة التسمية غير صالح
fm-cannot-rename-root = لا يمكن إعادة تسمية الجذر
fm-no-provider-path = تعذر الوصول إلى هذا المسار
fm-empty-tag = لا يمكن أن يكون اسم الوسم فارغًا
fm-drop-not-directory = هدف الإسقاط ليس مجلدًا
fm-drop-unavailable = هدف الإسقاط غير متاح
fm-type-ext-file = ملف { $ext }
fm-transfer-already-exists = يوجد ملف بهذا الاسم بالفعل
fm-transfer-virtual-dest = لا يمكن النسخ أو النقل إلى مجلد افتراضي
fm-clipboard-copy = { $count } إدخالات جاهزة للصق
fm-clipboard-cut = { $count } إدخالات (قص) جاهزة للصق
fm-sidebar-network = الشبكة
fm-sidebar-network-all = جميع الأماكن
fm-category-images = صور
fm-category-documents = مستندات
fm-category-video = فيديو
fm-category-audio = صوت
fm-category-archives = أرشيفات
fm-virtual-recent = الأخيرة
fm-virtual-starred = المميزة
fm-virtual-tags = الوسوم
fm-virtual-recent-empty = لا توجد ملفات حديثة. افتح ملفات لعرضها هنا.
fm-virtual-starred-empty = لا توجد ملفات مميزة. ميّز العناصر من قائمة السياق.
fm-virtual-tags-empty = لا توجد ملفات موسومة. أضف وسومًا من قائمة السياق.
fm-virtual-category-empty = لم يُعثر على ملفات مطابقة في هذه الفئة.
fm-virtual-create-denied = لا يمكن إنشاء مجلدات في موقع افتراضي
fm-empty-folder = هذا المجلد فارغ
fm-entry-encrypted-hint = Encrypted file
fm-entry-managed-hint = Managed folder
fm-error-access = تعذّر الوصول إلى هذا الموقع


# ==== Startup shell (task 11A) ====
window-title = Orchid
startup-welcome = مرحبًا بك في Orchid
startup-subtitle = بيئة حوسبة تُصمَّم للمس أولًا
startup-version-label = الإصدار { $version }
status-theme = السمة:
status-language = اللغة:
status-density = الكثافة:
density-touch = لمس
density-mouse = فأرة
density-hybrid = هجين

# ---- Workspace shell (task 11B) ----
startup-get-started = البدء
onboarding-back = رجوع
onboarding-next = التالي
onboarding-skip = تخطّي الجولة
onboarding-finish = البدء
onboarding-step-progress = الخطوة { $current } من { $total }
onboarding-step-welcome-title = مرحبًا بك في Orchid
onboarding-step-welcome-body = Orchid بيئة تُصمَّم للمس أولًا، حيث الإيماءات والأوامر وعناصر الواجهة ثلاثة أشكال لنفس الإجراء. هذه الجولة القصيرة تعرّفك بالأساسيات.
onboarding-step-workspace-title = مساحة عملك
onboarding-step-workspace-body = بدّل بين مساحات العمل في الأعلى، رتّب عناصر الواجهة على اللوحة، وأضف عناصر جديدة من الشريط السفلي.
onboarding-step-palette-title = لوحة الأوامر
onboarding-step-palette-body = اضغط Ctrl+Shift+P لتنفيذ أي أمر. كل عنصر يعرض اختصاره لتتعلّم أثناء الاستخدام.
onboarding-step-gestures-title = الإيماءات والتلميحات
onboarding-step-gestures-body = اسحب من حواف الشاشة لفتح اللوحات والشريط السفلي. اضغط Win+? في أي وقت لتبديل وضع التلميحات ورؤية ما هو متاح.
onboarding-hint-workspace = اسحب من الحافة اليسرى لمساحات العمل
onboarding-hint-dock = اسحب للأعلى من الحافة السفلية للشريط السفلي
onboarding-hint-gestures = Win+? يبدّل هذه التلميحات
workspace-default-name = الرئيسية
workspace-new = مساحة عمل جديدة
workspace-placement-blocked-title = تعذّر وضع الأداة هنا
workspace-placement-blocked-body = هذا الموضع يتداخل مع أداة أخرى أو يخرج عن الشبكة. جرّب خلية فارغة.
group-tooltip-dissolve = فك تجميع عناصر الواجهة
group-tooltip-move-left = نقل التبويب يساراً
group-tooltip-move-right = نقل التبويب يميناً
group-tooltip-close-tab = إزالة من المجموعة
group-hint-alt-detach = Alt+سحب لفصل عن المجموعة
workspace-unnamed = مساحة عمل { $n }
dock-add-label = إضافة عنصر واجهة
catalog-title = دليل عناصر الواجهة
catalog-search-placeholder = البحث في عناصر الواجهة…
dock-widget-terminal = الطرفية
dock-widget-weather = الطقس
dock-widget-moon = القمر
dock-widget-system = النظام
dock-widget-rss = الأخبار
dock-widget-recent-files = الأخيرة
dock-widget-search = البحث
dock-widget-media = الوسائط
dock-widget-password = كلمات المرور
dock-widget-viewer = العارض
dock-widget-fm = الملفات

viewer-no-file = لا يوجد ملف مفتوح
viewer-loading-path = جارٍ التحميل: { $path }
viewer-error-with-reason = تعذّر عرض هذا الملف: { $reason }
viewer-error-archive-entry-not-found = Archive entry not found
viewer-error-file-too-large = This file is too large to open
viewer-error-image-decode = Could not decode this image
viewer-error-parse-text = Could not read this text file
viewer-error-pdf-empty = This PDF has no pages
viewer-error-pdf-render = Could not render this PDF page
viewer-error-syntax-grammar = Syntax highlighting is unavailable for this language
viewer-error-thumbnail = Could not generate a thumbnail
viewer-pdf-unavailable = دعم PDF غير متاح في هذا الإصدار.
viewer-image-heic-unsupported = صور HEIC غير مدعومة بعد
viewer-image-raw-unsupported = صور RAW غير مدعومة بعد
viewer-archive-select-preview = حدّد ملفًا للمعاينة
viewer-archive-binary-preview = ملف ثنائي، { $size }

password-select-entry = حدّد إدخالًا
password-label-title = العنوان
password-label-username = اسم المستخدم
password-label-password = كلمة المرور
password-label-url = URL
password-label-notes = ملاحظات
password-label-totp = TOTP
password-action-lock = قفل
password-action-add = إضافة
password-add-title = إدخال جديد
password-add-submit = حفظ
password-add-cancel = إلغاء
password-generate = إنشاء
password-add-error-title = العنوان مطلوب
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
password-entry-added = تم حفظ الإدخال

password-username-copied = تم نسخ اسم المستخدم

moon-age-label = العمر
moon-distance-label = المسافة
moon-next-full-label = البدر التالي
moon-next-new-label = المحاق التالي
moon-moonrise-label = شروق القمر
moon-moonset-label = غروب القمر
moon-sunrise-label = شروق الشمس
moon-sunset-label = غروب الشمس
moon-libration-label = Libration

widget-title-terminal = الطرفية
widget-close-tooltip = إغلاق عنصر الواجهة
widget-close-confirm = إغلاق { $name }؟
action-confirm-yes = نعم
action-confirm-no = لا

fm-confirm-title = تأكيد
