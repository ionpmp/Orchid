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
rss-no-feeds = Ленты не настроены
rss-error-summary = Не удалось обновить { $n } из { $total } лент
rss-item-published-minutes = { $m } мин. назад
rss-item-published-hours = { $h } ч. назад
rss-item-published-days = { $d } дн. назад

# ---- Universal Search ----
search-placeholder = Введите текст для поиска файлов, команд и настроек…
search-empty-state = Начните вводить запрос
search-no-results = Ничего не найдено по запросу «{ $query }»
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
