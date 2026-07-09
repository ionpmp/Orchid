# Orchid Italian (it-IT) message catalog.
#
# Consumed by the upcoming `orchid-i18n::LocaleManager`. Until that lands,
# the built-in widgets fall back to the English strings baked into their
# Rust code — the keys below mirror those defaults and will become the
# single source of truth once the bundle loader is wired up.

# ---- Widget registry metadata ----
widget-terminal-name = Terminale
widget-terminal-desc = Shell locali, WSL o SSH con PTY, colori ANSI e scrollback

widget-weather-name = Meteo
widget-weather-desc = Condizioni attuali e previsioni a 3 giorni

widget-moon-name = Luna
widget-moon-desc = Fase lunare attuale, orari di sorgere/tramonto e dati celesti

widget-system-name = Sistema
widget-system-desc = Indicatori CPU, memoria, disco, rete e batteria
# ---- Shared size / duration formatting ----
byte-size-b = { $value } B
byte-size-kb = { $value } KB
byte-size-mb = { $value } MB
byte-size-gb = { $value } GB
byte-size-tb = { $value } TB
duration-days-hours = { $days }g { $hours }h
duration-hours-minutes = { $hours }h { $minutes }m
duration-minutes = { $minutes }m
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

widget-rss-name = Notizie
widget-rss-desc = Feed di notizie RSS e Atom

widget-recent-files-name = File recenti
widget-recent-files-desc = File aperti di recente in Orchid

widget-search-name = Ricerca universale
widget-search-desc = Cerca file, esegui comandi, apri impostazioni

widget-media-name = Lettore multimediale
widget-media-desc = Riproduzione in corso con controlli di trasporto

widget-password-name = Password
widget-password-desc = Accedi al database delle password

widget-viewer-name = Visualizzatore
widget-viewer-desc = Visualizza immagini, documenti, file sorgente e archivi

# ---- Weather ----
weather-condition-clear = Sereno
weather-condition-partly-cloudy = Parzialmente nuvoloso
weather-condition-cloudy = Nuvoloso
weather-condition-overcast = Coperto
weather-condition-fog = Nebbia
weather-condition-drizzle = Pioggerella
weather-condition-rain = Pioggia
weather-condition-snow = Neve
weather-condition-sleet = Nevischio
weather-condition-thunderstorm = Temporale
weather-condition-hail = Grandine
weather-condition-windy = Ventoso
weather-condition-unknown = Sconosciuto
weather-day-today = Oggi
weather-day-tomorrow = Domani
weather-status-fresh = Aggiornato
weather-status-stale = I dati potrebbero essere obsoleti
weather-status-offline = Offline
weather-status-error = Errore nel caricamento del meteo
weather-updated-just-now = Aggiornato ora
weather-updated-minutes = Aggiornato { $m } min fa
weather-updated-hours = Aggiornato { $h } h fa
weather-updated-days = Aggiornato { $d } g fa

# ---- Relative time (shared) ----
relative-just-now = adesso
relative-minutes = { $m } min fa
relative-hours = { $h } h fa
relative-days = { $d } g fa

weather-loading = Caricamento meteo…
weather-feels-like = Percepita { $temp }
weather-humidity-label = Umidità
weather-wind-label = Vento
weather-humidity-line = { $label } { $h }%
weather-wind-line = { $label } { $speed } km/h { $dir }
weather-wind-line-no-dir = { $label } { $speed } km/h

# ---- Wind directions ----
weather-wind-n = N
weather-wind-nne = NNE
weather-wind-ne = NE
weather-wind-ene = ENE
weather-wind-e = E
weather-wind-ese = ESE
weather-wind-se = SE
weather-wind-sse = SSE
weather-wind-s = S
weather-wind-ssw = SSO
weather-wind-sw = SO
weather-wind-wsw = OSO
weather-wind-w = O
weather-wind-wnw = ONO
weather-wind-nw = NO
weather-wind-nnw = NNO

# ---- Moon ----
moon-phase-new = Luna nuova
moon-phase-waxing-crescent = Luna crescente
moon-phase-first-quarter = Primo quarto
moon-phase-waxing-gibbous = Gibbosa crescente
moon-phase-full = Luna piena
moon-phase-waning-gibbous = Gibbosa calante
moon-phase-last-quarter = Ultimo quarto
moon-phase-waning-crescent = Luna calante
moon-illumination = { $pct }% illuminata
moon-age = Età: { $days } giorni
moon-distance = Distanza: { $km } km
moon-next-full = Prossima luna piena: { $date }
moon-next-new = Prossima luna nuova: { $date }
moon-moonrise = Sorgere della luna: { $time }
moon-moonset = Tramonto della luna: { $time }
moon-sunrise = Alba: { $time }
moon-sunset = Tramonto: { $time }
moon-libration = Libration: { $lat }°, { $lon }°
moon-loading = Calcolo dati lunari…

# ---- System ----
system-cpu-label = CPU
system-memory-label = Memoria
system-disk-label = Disco { $mount }
system-network-label = Rete { $name }
system-battery-label = Batteria
system-uptime-label = Tempo attivo
system-battery-charging = In carica
system-battery-time-remaining = { $time } rimanenti
system-network-rate = ↑ { $up }/s  ↓ { $down }/s
system-loading = Caricamento metriche di sistema…

# ---- RSS ----
rss-no-feeds = Nessun feed configurato
rss-loading = Caricamento notizie…
rss-fetch-failed = Impossibile caricare i feed. Controlla la connessione e riprova.
rss-empty = Nessun elemento nei feed configurati.
recent-files-empty = Nessun file recente. Apri file nel visualizzatore o nel gestore file per vederli qui.
recent-files-open-hint = Apri file
rss-open-item-hint = Apri collegamento
rss-error-summary = { $n } di { $total } feed non aggiornati
rss-item-published-minutes = { $m } min fa
rss-item-published-hours = { $h } h fa
rss-item-published-days = { $d } g fa

# ---- Universal Search ----
search-placeholder = Cerca file, comandi, impostazioni…
search-empty-state = Inizia a digitare per cercare
search-no-results = Nessun risultato per «{ $query }»
search-no-results-short = Nessun risultato
search-sources-unconfigured = Le fonti di ricerca non sono ancora configurate
search-searching = Ricerca…
search-source-files = File
search-source-commands = Comandi
search-source-settings = Impostazioni
command-terminal-invocation = orc { $verb }

# ---- Command palette ----
command-palette-placeholder = Esegui un comando…
command-palette-empty = Tutti i comandi

# ---- Registered commands ----
command.widget.create.name = Crea widget
command.widget.create.desc = Aggiungi un nuovo widget all'area di lavoro
command.widget.create.arg.type = Id tipo widget (es. terminal, weather)

command.widget.close.name = Chiudi widget
command.widget.close.desc = Chiudi un'istanza del widget

command.widget.move.name = Sposta widget
command.widget.resize.name = Ridimensiona widget
command.widget.focus_next.name = Widget successivo
command.widget.show_all.name = Mostra tutti i widget
command.widget.group.dissolve.name = Sciogli gruppo widget

command.workspace.create.name = Crea area di lavoro
command.workspace.delete.name = Elimina area di lavoro
command.workspace.switch_to.name = Passa all'area di lavoro
command.workspace.switch_next.name = Area di lavoro successiva
command.workspace.switch_previous.name = Area di lavoro precedente

command.terminal.split_horizontal.name = Dividi terminale orizzontalmente
command.terminal.split_vertical.name = Dividi terminale verticalmente
command.terminal.tab_new.name = Nuova scheda terminale
command.terminal.close.name = Chiudi riquadro o scheda terminale
command.terminal.focus_next_pane.name = Riquadro terminale successivo
command.terminal.focus_previous_pane.name = Riquadro terminale precedente
command.terminal.tab_next.name = Scheda terminale successiva
command.terminal.tab_previous.name = Scheda terminale precedente

# ---- Settings (universal search) ----
settings.section.general = Generale
settings.section.appearance = Aspetto
settings.section.input = Input
settings.section.shortcuts = Scorciatoie
settings.section.locale = Lingua
settings.section.privacy = Privacy

# ---- Settings panel ----
settings-panel-title = Impostazioni
settings-panel-hint = I valori sono di sola lettura per ora. Modifica config.toml direttamente; le modifiche si ricaricano automaticamente.
settings-panel-coming-soon = L'editor completo delle impostazioni per questa sezione non è ancora disponibile. Modifica config.toml direttamente per ora.
settings-panel-ok = Chiudi

settings-open-in-editor = Apri nell'editor
settings-open-config-file = Apri config.toml
settings-value-none = Nessuno
settings-value-leader-timeout = { $ms } ms
settings-shortcut-binding = { $key } → { $cmd }
settings-shortcut-list-separator = , 
settings-value-default = Predefinito
settings-value-disabled = Disabilitato
settings-value-system-default = Predefinito di sistema
settings-value-hand-left = Sinistra
settings-value-hand-right = Destra
settings-value-pen-double-tap-none = Nessuno
settings-value-pen-double-tap-switch-tool = Cambia strumento
settings-value-pen-double-tap-erase = Cancella
settings-value-sunday = Domenica
settings-value-monday = Lunedì

settings-field-auto-update = Aggiornamento automatico
settings-field-telemetry = Telemetria
settings-field-open-on-startup = Apri all'avvio
settings-field-theme = Tema
settings-field-density = Densità
settings-field-font-family = Famiglia di caratteri
settings-field-font-scale = Scala caratteri
settings-field-reduce-motion = Riduci animazioni
settings-field-follow-system-theme = Segui tema di sistema
settings-field-dark-theme = Tema scuro
settings-field-light-theme = Tema chiaro
settings-field-primary-hand = Mano dominante
settings-field-mirror-edge-swipes = Inverti scorrimenti sui bordi
settings-field-haptic-feedback = Feedback aptico
settings-field-palm-rejection = Rifiuto del palmo
settings-field-pen-double-tap = Doppio tocco penna
settings-field-shortcut-overrides = Scorciatoie personalizzate
settings-field-leader-key = Tasto leader
settings-field-leader-timeout = Timeout leader
settings-field-leader-bindings = Associazioni leader
settings-field-language = Lingua
settings-field-date-format = Formato data
settings-field-time-format = Formato ora
settings-field-first-day-of-week = Primo giorno della settimana
settings-field-record-action-history = Registra cronologia azioni
settings-field-history-retention-days = Conservazione cronologia (giorni)
settings-field-clear-clipboard-seconds = Svuota appunti dopo copia

settings-field-vault-auto-lock = Blocco automatico del vault (secondi)
command.settings.open.name = Apri impostazioni
command.settings.open.desc = Mostra il pannello impostazioni
command.settings.open_config_file.name = Apri config
command.settings.open_config_file.desc = Apri config.toml nell'editor predefinito
command.password.lock.name = Blocca cassaforte password
command.password.lock.desc = Rimuovi il database password sbloccato dalla memoria

command.navigation.show_workspace_panel.name = Mostra pannello spazi di lavoro
command.navigation.show_workspace_panel.desc = Mostra o nascondi la barra laterale degli spazi di lavoro
command.notification.show_center.name = Mostra centro notifiche
command.notification.show_center.desc = Mostra o nascondi il centro notifiche
command.dock.show.name = Mostra dock
command.dock.show.desc = Mostra o nascondi il dock dei widget
command.search.show_universal.name = Ricerca universale
command.search.show_universal.desc = Apri o metti a fuoco la ricerca universale
command.onboarding.toggle_hint_mode.name = Attiva/disattiva modalità suggerimenti
command.onboarding.toggle_hint_mode.desc = Mostra o nascondi i suggerimenti sui gesti nell'area di lavoro
navigation-workspace-panel-title = Spazi di lavoro
notification-center-title = Notifiche
notification-center-placeholder = Nessuna notifica per ora.
notification-center-clear = Cancella tutto
notification-center-dismiss = Ignora
notification-center-tip-title = Suggerimento
notification-center-tip-body = Scorri dal bordo destro o esegui «Mostra centro notifiche» per aprire questo pannello.
# ---- Terminal tab bar ----
terminal-tooltip-split-h = Dividi orizzontalmente (Ctrl+Shift+H)
terminal-tooltip-split-v = Dividi verticalmente (Ctrl+Shift+J)
terminal-tooltip-tab-new = Nuova scheda (Ctrl+Shift+T)
terminal-tooltip-tab-close = Chiudi scheda
terminal-tooltip-pane-close = Chiudi riquadro

# ---- Media player ----
media-no-session = Nessuna riproduzione
media-loading = Caricamento media…
media-unsupported = I controlli multimediali non sono disponibili su questa piattaforma
media-play = Riproduci
media-pause = Pausa
media-next = Successivo
media-previous = Precedente

# ---- Password manager ----
password-locked = Il database è bloccato
password-unlock-label = Password principale
password-unlock-placeholder = Inserisci password principale
password-unlock-submit = Sblocca
password-unlock-biometric = Windows Hello
password-unlock-biometric-prompt = Sblocca cassaforte password
password-search-placeholder = Cerca voci…
password-no-entries = Nessuna voce
password-copy-password = Copia password
password-copy-username = Copia nome utente
password-copy-totp = Copia TOTP
password-open-url = Apri URL
password-password-copied = Password copiata (si cancella in 30 s)
password-totp-copied = TOTP copiato (si cancella in 30 s)
password-totp-remaining = { $s } s


# ==== Viewer widget ====
widget-viewer-name = Visualizzatore
widget-viewer-desc = Apri file: immagini, PDF, testo, archivi
viewer-loading = Caricamento…
viewer-error = Impossibile visualizzare questo file
viewer-unsupported = Tipo di file non supportato
viewer-image-fit-screen = Adatta allo schermo
viewer-image-actual-size = Dimensione reale
viewer-image-rotate = Ruota
viewer-image-flip-h = Capovolgi orizzontalmente
viewer-image-flip-v = Capovolgi verticalmente
viewer-image-zoom-in = Ingrandisci
viewer-image-zoom-out = Riduci
viewer-image-rotate-cw = Ruota in senso orario
viewer-image-rotate-ccw = Ruota in senso antiorario
viewer-archive-root = (radice)
viewer-archive-parent = Cartella superiore
viewer-pdf-page-of = Pagina { $current } di { $total }
viewer-pdf-fit-width = Adatta alla larghezza
viewer-pdf-fit-page = Adatta alla pagina
viewer-pdf-go = Vai
viewer-pdf-prev-page = Pagina precedente
viewer-pdf-next-page = Pagina successiva
viewer-pdf-info = PDF · pag. { $current } / { $total } · { $width } × { $height } px · { $zoom }%
viewer-image-info = { $width } × { $height } · { $size } · { $format }
viewer-archive-info = { $format }, { $count } voci
viewer-archive-extracted-selected = Estratto in { $path }
viewer-archive-extracted-all = Estratte { $count } voci in { $path }
viewer-archive-nothing-selected = Nessun elemento selezionato da estrarre
viewer-archive-cannot-extract-folder = Impossibile estrarre una cartella
viewer-action-failed = Azione del visualizzatore non riuscita: { $reason }
viewer-text-save-failed = Impossibile salvare il file: { $reason }
viewer-text-read-only = Sola lettura
viewer-text-editing = Modifica
viewer-text-save = Salva (Ctrl+S)
viewer-text-lines = { $count } righe
viewer-text-line-ending-lf = LF
viewer-text-line-ending-crlf = CRLF
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
viewer-text-unsaved-title = Modifiche non salvate
viewer-text-unsaved-body = Salvare le modifiche prima di chiudere?
viewer-text-discard = Scarta

viewer-text-dirty-indicator = Modifiche non salvate
viewer-archive-extract-all = Estrai tutto
viewer-archive-extract-selected = Estrai selezione

# ==== File manager widget ====
widget-fm-name = File
widget-fm-desc = Sfoglia, organizza e gestisci i file
fm-nav-back = Indietro
fm-nav-forward = Avanti
fm-nav-up = Su
fm-nav-home = Home
fm-view-icons = Icone
fm-view-list = Elenco
fm-view-details = Dettagli
fm-view-gallery = Galleria
fm-sort-name = Nome
fm-sort-size = Dimensione
fm-sort-modified = Modificato
fm-sort-type = Tipo
fm-action-open = Apri
fm-action-open-all = Apri tutto
fm-action-open-with = Apri con…
fm-action-open-default = Apri con app predefinita
fm-action-open-in-viewer = Apri in Orchid Viewer
fm-action-copy = Copia
fm-action-cut = Taglia
fm-action-paste = Incolla
fm-action-rename = Rinomina
fm-action-delete = Elimina
fm-action-new-folder = Nuova cartella
fm-action-new-tab = Nuova scheda
fm-action-close-tab = Chiudi scheda
fm-action-select-all = Seleziona tutto
fm-action-deselect-all = Deseleziona tutto
fm-action-star = Preferito
fm-action-unstar = Rimuovi preferito
fm-action-encrypt = Cifra
fm-action-reveal = Mostra temporaneamente
fm-action-decrypt = Decifra
fm-action-add-tag = Aggiungi tag…
fm-action-remove-tag = Rimuovi tag
fm-action-color-label = Etichetta colore
fm-color-red = Rosso
fm-color-orange = Arancione
fm-color-yellow = Giallo
fm-color-green = Verde
fm-color-blue = Blu
fm-color-purple = Viola
fm-color-gray = Grigio
fm-color-none = Nessun colore
fm-action-properties = Proprietà
fm-action-add-to-managed = Aggiungi a cartella gestita
fm-action-remove-from-managed = Rimuovi da cartelle gestite
fm-action-managed-policy = Politica cartella gestita
fm-managed-policy-title = Politica cartella gestita
fm-policy-max-size = Dimensione max
fm-policy-retention = Conservazione
fm-policy-excludes = Pattern di esclusione
fm-policy-unlimited = Illimitato
fm-policy-forever = Conserva per sempre
fm-policy-retention-days = { $days } giorni
fm-policy-none = Nessuno
fm-sidebar-managed-folder-policy = { $name } ({ $count } file, { $dedup } risparmiati, politica)
fm-sidebar-managed-policy-only = { $name } (politica)
fm-rename-title = Rinomina
fm-rename-ok = OK
fm-rename-cancel = Annulla
fm-dual-pane-on = Doppio riquadro
fm-dual-pane-off = Riquadro singolo
fm-show-hidden-on = Mostra file nascosti
fm-show-hidden-off = Nascondi file nascosti
fm-click-single-on = Un clic per aprire
fm-click-single-off = Doppio clic per aprire
fm-encrypt-title = Cifra con passphrase
fm-reveal-title = Inserisci passphrase per mostrare
fm-decrypt-title = Inserisci passphrase per decifrare
fm-info-close = Chiudi
fm-properties-title = Proprietà
fm-properties-kind-folder = Cartella
fm-properties-kind-file = File
fm-properties-type = Tipo: { $kind }
fm-properties-size = Dimensione: { $size }
fm-properties-modified = Modificato: { $modified }
fm-properties-mime = MIME: { $mime }
fm-tag-add-title = Aggiungi tag
fm-confirm-delete = Eliminare { $n } elementi?
fm-confirm-delete-permanent = Eliminare definitivamente { $n } elementi?
fm-loading = Caricamento…
fm-status-bar = { $items } elementi, { $selected } selezionati
fm-status-managed = { $items } elementi, { $selected } selezionati · { $tracked } ingeriti, { $dedup } deduplicati
fm-encrypted = Cifrato: { $name }
fm-decrypted = Decifrato: { $name }
fm-managed-added = Aggiunto a cartella gestita
fm-managed-removed = Rimosso da cartelle gestite
fm-encryption-unavailable = La cifratura non è disponibile
fm-passphrase-failed = Passphrase non riuscita: { $reason }
fm-passphrase-invalid = Passphrase non valida
fm-passphrase-required = Passphrase obbligatoria
fm-decryption-failed = Decifratura non riuscita
fm-passphrase-encrypt-hint = Scegli una passphrase robusta. Non può essere recuperata se persa.
fm-passphrase-decrypt-hint = Inserisci la passphrase usata per cifrare questi file.
fm-passphrase-reveal-hint = I file vengono decifrati in una posizione temporanea per la visualizzazione.
fm-passphrase-biometric = Windows Hello
fm-passphrase-biometric-prompt = Sblocca file cifrati
fm-revealed = Mostrato: { $name }
fm-managed-unavailable = Le cartelle gestite non sono disponibili
fm-managed-no-selection = Seleziona una cartella da aggiungere alle cartelle gestite
fm-not-managed-folder = Non è una cartella gestita
fm-managed-conflict = Conflitto cartella gestita
fm-sidebar-managed-folder = { $name } ({ $count } file, { $dedup } risparmiati)
fm-ingest-failed = Ingestione non riuscita: { $name }
fm-quick-filter-placeholder = Filtra…
fm-sidebar-favorites = Preferiti
fm-sidebar-categories = Categorie
fm-sidebar-managed = Cartelle gestite
fm-network-placeholder = Nessun mount di rete configurato. Aggiungi voci [[file-manager.network-mounts]] in config.toml (SFTP, SMB, WebDAV, FTP via rclone).
fm-network-no-provider = Nessun provider di filesystem registrato per questa posizione di rete.
fm-network-rclone-missing = rclone non è installato o non è nel PATH. Imposta RCLONE_BIN se necessario.
fm-network-invalid-mount = Questo mount di rete è configurato male. Controlla nome e URI in config.toml.
fm-network-auth-failed = Autenticazione non riuscita. Controlla username e password in config.toml.
fm-network-permission-denied = Permesso negato su questa posizione di rete.
fm-network-connection-failed = Impossibile connettersi all'host di rete. Controlla URI e rete.
fm-ingested = Ingerito: { $name }
fm-ingesting = Ingestione: { $name } ({ $count } attivi)
fm-ingesting-count = Ingestione di { $count } file…
fm-copying = Copia: { $name } ({ $percent }%)
fm-moving = Spostamento: { $name } ({ $percent }%)
fm-transfer-failed = Trasferimento non riuscito: { $reason }
fm-action-failed = Azione sul file non riuscita: { $reason }
fm-invalid-folder-name = Nome cartella non valido
fm-no-provider-parent = Impossibile accedere alla cartella padre
fm-no-parent-folder = Nessuna cartella padre
fm-selection-multiple-folders = La selezione include più cartelle
fm-invalid-rename-target = Destinazione di rinomina non valida
fm-cannot-rename-root = Impossibile rinominare la radice
fm-no-provider-path = Impossibile accedere a questo percorso
fm-empty-tag = Il nome del tag non può essere vuoto
fm-drop-not-directory = La destinazione non è una cartella
fm-drop-unavailable = Destinazione non disponibile
fm-type-ext-file = File { $ext }
fm-transfer-already-exists = Esiste già un file con quel nome
fm-transfer-virtual-dest = Impossibile copiare o spostare in una cartella virtuale
fm-clipboard-copy = { $count } voci pronte per incollare
fm-clipboard-cut = { $count } voci (taglia) pronte per incollare
fm-sidebar-network = Rete
fm-sidebar-network-all = Tutte le posizioni
fm-category-images = Immagini
fm-category-documents = Documenti
fm-category-video = Video
fm-category-audio = Audio
fm-category-archives = Archivi
fm-virtual-recent = Recenti
fm-virtual-starred = Preferiti
fm-virtual-tags = Tag
fm-virtual-recent-empty = Nessun file recente. Apri file per vederli qui.
fm-virtual-starred-empty = Nessun preferito. Aggiungi preferiti dal menu contestuale.
fm-virtual-tags-empty = Nessun file con tag. Aggiungi tag dal menu contestuale.
fm-virtual-category-empty = Nessun file corrispondente in questa categoria.
fm-virtual-create-denied = Impossibile creare cartelle in una posizione virtuale
fm-empty-folder = Questa cartella è vuota
fm-error-access = Impossibile accedere a questa posizione


# ==== Startup shell (task 11A) ====
window-title = Orchid
startup-welcome = Benvenuto in Orchid
startup-subtitle = Un ambiente informatico pensato per il touch
startup-version-label = Versione { $version }
status-theme = Tema:
status-language = Lingua:
status-density = Densità:
density-touch = Touch
density-mouse = Mouse
density-hybrid = Ibrido

# ---- Workspace shell (task 11B) ----
startup-get-started = Inizia
onboarding-back = Indietro
onboarding-next = Avanti
onboarding-skip = Salta tour
onboarding-finish = Inizia
onboarding-step-progress = Passo { $current } di { $total }
onboarding-step-welcome-title = Benvenuto in Orchid
onboarding-step-welcome-body = Orchid è un ambiente touch-first in cui gesti, comandi e widget sono tre forme della stessa azione. Questo breve tour mostra l'essenziale.
onboarding-step-workspace-title = Il tuo spazio di lavoro
onboarding-step-workspace-body = Cambia spazio di lavoro in alto, disporre i widget sulla tela e aggiungerne di nuovi dal dock in basso.
onboarding-step-palette-title = Palette comandi
onboarding-step-palette-body = Premi Ctrl+Shift+P per eseguire un comando. Ogni voce mostra la scorciatoia da tastiera così puoi imparare mentre lavori.
onboarding-step-gestures-title = Gesti e suggerimenti
onboarding-step-gestures-body = Scorri dai bordi dello schermo per pannelli e dock. Premi Win+? in qualsiasi momento per attivare la modalità suggerimenti e vedere cosa è disponibile.
onboarding-hint-workspace = Scorri dal bordo sinistro per gli spazi di lavoro
onboarding-hint-dock = Scorri verso l'alto dal bordo inferiore per il dock
onboarding-hint-gestures = Win+? attiva/disattiva questi suggerimenti
workspace-default-name = Principale
workspace-new = Nuova area di lavoro
workspace-placement-blocked-title = Impossibile posizionare il widget qui
workspace-placement-blocked-body = Lo spazio si sovrappone a un altro widget o esce dalla griglia. Prova una cella libera.
group-tooltip-dissolve = Separa i widget
group-tooltip-move-left = Sposta scheda a sinistra
group-tooltip-move-right = Sposta scheda a destra
group-tooltip-close-tab = Rimuovi dal gruppo
group-hint-alt-detach = Alt+trascina per staccare dal gruppo
workspace-unnamed = Area di lavoro { $n }
dock-add-label = Aggiungi widget
catalog-title = Catalogo widget
catalog-search-placeholder = Cerca widget…
dock-widget-terminal = Terminale
dock-widget-weather = Meteo
dock-widget-moon = Luna
dock-widget-system = Sistema
dock-widget-rss = Notizie
dock-widget-recent-files = Recenti
dock-widget-search = Ricerca
dock-widget-media = Media
dock-widget-password = Password
dock-widget-viewer = Visualizzatore
dock-widget-fm = File

viewer-no-file = Nessun file aperto
viewer-loading-path = Caricamento: { $path }
viewer-error-with-reason = Impossibile visualizzare questo file: { $reason }
viewer-pdf-unavailable = Il supporto PDF non è disponibile in questa build.
viewer-image-heic-unsupported = Le immagini HEIC non sono ancora supportate
viewer-image-raw-unsupported = Le immagini RAW non sono ancora supportate
viewer-archive-select-preview = Seleziona un file da anteprima
viewer-archive-binary-preview = File binario, { $size }

password-select-entry = Seleziona una voce
password-label-title = Titolo
password-label-username = Nome utente
password-label-password = Password
password-label-url = URL
password-label-notes = Note
password-label-totp = TOTP
password-action-lock = Blocca
password-action-add = Aggiungi
password-add-title = Nuova voce
password-add-submit = Salva
password-add-cancel = Annulla
password-generate = Genera
password-add-error-title = Il titolo è obbligatorio
password-entry-added = Voce salvata

password-username-copied = Nome utente copiato

moon-age-label = Età
moon-distance-label = Distanza
moon-next-full-label = Prossima luna piena
moon-next-new-label = Prossima luna nuova
moon-moonrise-label = Sorgere della luna
moon-moonset-label = Tramonto della luna
moon-sunrise-label = Alba
moon-sunset-label = Tramonto
moon-libration-label = Libration

widget-title-terminal = Terminale
widget-close-tooltip = Chiudi widget
widget-close-confirm = Chiudere { $name }?
action-confirm-yes = Sì
action-confirm-no = No

fm-confirm-title = Conferma
