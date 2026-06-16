# Orchid Russian (ru-RU) message catalog.

# ---- Widget registry metadata ----
widget-terminal-name = Терминал
widget-terminal-desc = Локальные, WSL или SSH оболочки с PTY, ANSI-цветами и историей

widget-weather-name = Погода
widget-weather-desc = Текущие условия и прогноз на 3 дня

widget-moon-name = Луна
widget-moon-desc = Текущая фаза Луны, время восхода/захода и астрономические данные

widget-system-name = Система
widget-system-desc = Индикаторы CPU, памяти, диска, сети и батареи

widget-rss-name = Новости
widget-rss-desc = Ленты RSS и Atom

widget-search-name = Универсальный поиск
widget-search-desc = Поиск файлов, команд и настроек

widget-media-name = Медиаплеер
widget-media-desc = Сейчас играет, управление воспроизведением

widget-password-name = Пароли
widget-password-desc = Доступ к базе паролей

widget-viewer-name = Просмотрщик
widget-viewer-desc = Просмотр изображений, документов, кода и архивов

# ---- Weather ----
weather-condition-clear = Ясно
weather-condition-partly-cloudy = Переменная облачность
weather-condition-cloudy = Облачно
weather-condition-overcast = Пасмурно
weather-condition-fog = Туман
weather-condition-drizzle = Моросящий дождь
weather-condition-rain = Дождь
weather-condition-snow = Снег
weather-condition-sleet = Мокрый снег
weather-condition-thunderstorm = Гроза
weather-condition-hail = Град
weather-condition-windy = Ветрено
weather-condition-unknown = Неизвестно
weather-day-today = Сегодня
weather-day-tomorrow = Завтра
weather-status-fresh = Актуально
weather-status-stale = Данные могут быть устаревшими
weather-status-offline = Офлайн
weather-status-error = Ошибка загрузки погоды
weather-updated-just-now = Обновлено только что
weather-updated-minutes = Обновлено { $m } мин. назад
weather-updated-hours = Обновлено { $h } ч. назад
weather-humidity-label = Влажность
weather-wind-label = Ветер

# ---- Moon ----
moon-phase-new = Новолуние
moon-phase-waxing-crescent = Молодая луна
moon-phase-first-quarter = Первая четверть
moon-phase-waxing-gibbous = Прибывающая луна
moon-phase-full = Полнолуние
moon-phase-waning-gibbous = Убывающая луна
moon-phase-last-quarter = Последняя четверть
moon-phase-waning-crescent = Старая луна
moon-illumination = Освещённость { $pct }%
moon-age = Возраст: { $days } дн.
moon-distance = Расстояние: { $km } км
moon-next-full = Ближайшее полнолуние: { $date }
moon-next-new = Ближайшее новолуние: { $date }
moon-moonrise = Восход Луны: { $time }
moon-moonset = Заход Луны: { $time }
moon-sunrise = Восход Солнца: { $time }
moon-sunset = Заход Солнца: { $time }
moon-libration = Либрация: { $lat }°, { $lon }°

# ---- System ----
system-cpu-label = ЦП
system-memory-label = Память
system-disk-label = Диск { $mount }
system-network-label = Сеть { $name }
system-battery-label = Батарея
system-uptime-label = Аптайм
system-battery-charging = Заряжается
system-battery-time-remaining = осталось { $time }
system-network-rate = ↑ { $up }/с  ↓ { $down }/с

# ---- RSS ----
rss-no-feeds = Нет настроенных лент
rss-error-summary = Не удалось обновить { $n } из { $total } лент
rss-item-published-minutes = { $m } мин. назад
rss-item-published-hours = { $h } ч. назад
rss-item-published-days = { $d } дн. назад

# ---- Universal Search ----
search-placeholder = Введите запрос для поиска файлов, команд и настроек...
search-empty-state = Начните вводить запрос
search-no-results = Ничего не найдено по запросу «{ $query }»
search-no-results-short = Нет результатов
search-searching = Идёт поиск…
search-source-files = Файлы
search-source-commands = Команды
search-source-settings = Настройки

# ---- Media player ----
media-no-session = Нет активного воспроизведения
media-play = Воспроизвести
media-pause = Пауза
media-next = Следующий
media-previous = Предыдущий

# ---- Password manager ----
password-locked = База паролей заблокирована
password-search-placeholder = Поиск записей…
password-no-entries = Записей пока нет
password-copy-password = Скопировать пароль
password-copy-username = Скопировать логин
password-copy-totp = Скопировать TOTP
password-open-url = Открыть URL
password-password-copied = Пароль скопирован (будет очищен через 30 с)
password-totp-copied = TOTP скопирован (будет очищен через 30 с)
password-totp-remaining = { $s } с


# ==== Viewer widget ====
viewer-loading = Загрузка…
viewer-error = Не удаётся отобразить файл
viewer-unsupported = Неподдерживаемый тип файла
viewer-image-fit-screen = По размеру экрана
viewer-image-actual-size = Реальный размер
viewer-image-rotate = Повернуть
viewer-image-flip-h = Отразить по горизонтали
viewer-image-flip-v = Отразить по вертикали
viewer-pdf-page-of = Страница { $current } из { $total }
viewer-pdf-fit-width = По ширине
viewer-pdf-fit-page = По странице
viewer-text-read-only = Только чтение
viewer-text-editing = Редактирование
viewer-text-save = Сохранить
viewer-text-dirty-indicator = Несохранённые изменения
viewer-archive-extract-all = Извлечь всё
viewer-archive-extract-selected = Извлечь выбранные
viewer-archive-preview-binary = Бинарный файл, { $size }

# ==== File manager widget ====
widget-fm-name = Файлы
widget-fm-desc = Обзор, организация и управление файлами
fm-nav-back = Назад
fm-nav-forward = Вперёд
fm-nav-up = Вверх
fm-nav-home = Домой
fm-view-icons = Иконки
fm-view-list = Список
fm-view-details = Подробно
fm-view-gallery = Галерея
fm-sort-name = Имя
fm-sort-size = Размер
fm-sort-modified = Изменён
fm-sort-type = Тип
fm-action-open = Открыть
fm-action-open-all = Открыть все
fm-action-open-with = Открыть с помощью…
fm-action-open-default = Открыть в приложении по умолчанию
fm-action-open-in-viewer = Открыть в просмотрщике Orchid
fm-action-copy = Копировать
fm-action-cut = Вырезать
fm-action-paste = Вставить
fm-action-rename = Переименовать
fm-action-delete = Удалить
fm-action-new-folder = Новая папка
fm-action-new-tab = Новая вкладка
fm-action-close-tab = Закрыть вкладку
fm-action-select-all = Выделить все
fm-action-deselect-all = Снять выделение
fm-action-star = В избранное
fm-action-unstar = Убрать из избранного
fm-action-encrypt = Зашифровать
fm-action-reveal = Временно открыть
fm-action-decrypt = Расшифровать
fm-action-add-tag = Добавить тег…
fm-action-remove-tag = Удалить тег
fm-action-color-label = Цветовая метка
fm-color-red = Красный
fm-color-orange = Оранжевый
fm-color-yellow = Жёлтый
fm-color-green = Зелёный
fm-color-blue = Синий
fm-color-purple = Фиолетовый
fm-color-gray = Серый
fm-color-none = Без цвета
fm-action-properties = Свойства
fm-action-add-to-managed = Добавить в управляемую папку
fm-action-remove-from-managed = Убрать из управляемых папок
fm-rename-title = Переименовать
fm-rename-ok = OK
fm-rename-cancel = Отмена
fm-dual-pane-on = Две панели
fm-dual-pane-off = Одна панель
fm-show-hidden-on = Показать скрытые
fm-show-hidden-off = Скрыть скрытые
fm-click-single-on = Открытие одним кликом
fm-click-single-off = Открытие двойным кликом
fm-encrypt-title = Зашифровать с паролем
fm-reveal-title = Введите пароль для временного открытия
fm-decrypt-title = Введите пароль для расшифровки
fm-info-close = Закрыть
fm-properties-title = Свойства
fm-tag-add-title = Добавить тег
fm-confirm-delete = Удалить { $n } элементов?
fm-confirm-delete-permanent = Удалить { $n } элементов безвозвратно?
fm-status-items = { $n } элементов
fm-status-selected = { $n } выделено
fm-status-total-size = { $size }
fm-status-bar = { $items } элементов, { $selected } выделено
fm-status-managed = { $items } элементов, { $selected } выделено · { $tracked } загружено, { $dedup } сэкономлено
fm-encrypted = Зашифровано: { $name }
fm-decrypted = Расшифровано: { $name }
fm-managed-added = Добавлено в управляемую папку
fm-managed-removed = Удалено из управляемых папок
fm-encryption-unavailable = Шифрование недоступно
fm-passphrase-failed = Ошибка пароля: { $reason }
fm-passphrase-invalid = Неверный пароль
fm-passphrase-required = Введите пароль
fm-decryption-failed = Не удалось расшифровать
fm-passphrase-encrypt-hint = Придумайте надёжный пароль. Его нельзя восстановить при потере.
fm-passphrase-decrypt-hint = Введите пароль, которым были зашифрованы эти файлы.
fm-passphrase-reveal-hint = Файлы расшифровываются во временную папку для просмотра.
fm-revealed = Открыто временно: { $name }
fm-managed-unavailable = Управляемые папки недоступны
fm-managed-no-selection = Выберите папку для добавления в управляемые
fm-not-managed-folder = Не управляемая папка
fm-managed-conflict = Конфликт управляемых папок
fm-sidebar-managed-folder = { $name } ({ $count } файлов, { $dedup } сэкономлено)
fm-ingest-failed = Ошибка загрузки: { $name }
fm-quick-filter-placeholder = Фильтр…
fm-sidebar-favorites = Избранное
fm-sidebar-categories = Категории
fm-sidebar-managed = Управляемые папки
fm-network-placeholder = Сетевые подключения ещё не настроены. Планируется поддержка SFTP, SMB и WebDAV через rclone.
fm-network-no-provider = Для этого сетевого расположения не зарегистрирован провайдер файловой системы.
fm-network-rclone-missing = rclone не установлен или не найден в PATH. При необходимости задайте RCLONE_BIN.
fm-network-invalid-mount = Сетевой mount настроен неверно. Проверьте имя и URI в config.toml.
fm-network-auth-failed = Ошибка аутентификации. Проверьте логин и пароль в config.toml.
fm-network-permission-denied = Нет доступа к этой сетевой папке.
fm-network-connection-failed = Не удалось подключиться к хосту. Проверьте URI и сеть.
fm-ingested = Загружено: { $name }
fm-ingesting = Загрузка: { $name } ({ $count } активно)
fm-ingesting-count = Загрузка { $count } файлов…
fm-copying = Копирование: { $name } ({ $percent }%)
fm-moving = Перемещение: { $name } ({ $percent }%)
fm-transfer-failed = Ошибка передачи: { $reason }
fm-transfer-already-exists = Файл с таким именем уже существует
fm-transfer-virtual-dest = Нельзя копировать или перемещать в виртуальную папку
fm-clipboard-copy = { $count } объектов готово к вставке
fm-clipboard-cut = { $count } объектов (вырезано) готово к вставке
fm-sidebar-tags = Теги
fm-sidebar-recent = Недавние
fm-sidebar-network = Сеть
fm-sidebar-network-all = Все места
fm-category-images = Изображения
fm-category-documents = Документы
fm-category-video = Видео
fm-category-audio = Аудио
fm-category-archives = Архивы
fm-virtual-recent = Недавние
fm-virtual-starred = Избранное
fm-virtual-tags = Теги
fm-virtual-recent-empty = Недавних файлов пока нет. Откройте файлы — они появятся здесь.
fm-virtual-starred-empty = Избранных файлов пока нет. Отметьте звёздочкой через контекстное меню.
fm-virtual-tags-empty = Файлов с тегами пока нет. Добавьте теги через контекстное меню.
fm-virtual-category-empty = В этой категории подходящих файлов не найдено.
fm-virtual-create-denied = Нельзя создавать папки в виртуальном расположении
fm-empty-folder = Папка пуста
fm-error-access = Нет доступа к этому расположению


# ==== Startup shell (task 11A) ====
window-title = Orchid
startup-welcome = Добро пожаловать в Orchid
startup-subtitle = Среда с приоритетом сенсорного ввода
startup-version-label = Версия { $version }
status-theme = Тема:
status-language = Язык:
status-density = Плотность:
density-touch = Сенсор
density-mouse = Мышь
density-hybrid = Смешанная

# ---- Workspace shell (task 11B) ----
startup-get-started = Начать работу
workspace-default-name = Главный
workspace-new = Новый рабочий стол
workspace-unnamed = Рабочий стол { $n }
dock-add-label = Добавить виджет
catalog-title = Каталог виджетов
catalog-search-placeholder = Поиск виджета…
dock-widget-terminal = Терминал
dock-widget-weather = Погода
dock-widget-moon = Луна
dock-widget-system = Система
dock-widget-rss = Новости
dock-widget-search = Поиск
dock-widget-media = Медиа
dock-widget-password = Пароли
dock-widget-viewer = Просмотрщик
dock-widget-fm = Файлы

viewer-no-file = Файл не открыт
viewer-loading-path = Загрузка: { $path }
viewer-error-with-reason = Не удаётся отобразить файл: { $reason }
viewer-pdf-unavailable = Поддержка PDF недоступна в этой сборке.
viewer-archive-select-preview = Выберите файл для предпросмотра
viewer-archive-binary-preview = Бинарный файл, { $size }

password-select-entry = Выберите запись
password-label-username = Имя пользователя
password-label-password = Пароль
password-label-url = URL
password-label-notes = Заметки
password-label-totp = TOTP
password-action-copy = Копировать
password-action-open = Открыть

password-username-copied = Имя пользователя скопировано

moon-age-label = Возраст
moon-distance-label = Расстояние
moon-next-full-label = Следующее полнолуние
moon-next-new-label = Следующее новолуние
moon-moonrise-label = Восход Луны
moon-moonset-label = Заход Луны
moon-sunrise-label = Восход Солнца
moon-sunset-label = Заход Солнца
moon-libration-label = Либрация

widget-title-terminal = Терминал
widget-close-tooltip = Закрыть виджет
widget-close-confirm = Закрыть { $name }?
action-confirm-yes = Да
action-confirm-no = Нет

fm-confirm-title = Подтверждение