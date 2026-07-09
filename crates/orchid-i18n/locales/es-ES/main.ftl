# Orchid Spanish (es-ES) message catalog.
#
# Consumed by the upcoming `orchid-i18n::LocaleManager`. Until that lands,
# the built-in widgets fall back to the English strings baked into their
# Rust code — the keys below mirror those defaults and will become the
# single source of truth once the bundle loader is wired up.

# ---- Widget registry metadata ----
widget-terminal-name = Terminal
widget-terminal-desc = Shells locales, WSL o SSH con PTY, colores ANSI e historial

widget-weather-name = Tiempo
widget-weather-desc = Condiciones actuales y previsión de 3 días

widget-moon-name = Luna
widget-moon-desc = Fase lunar actual, horas de salida/puesta y datos celestes

widget-system-name = Sistema
widget-system-desc = Indicadores de CPU, memoria, disco, red y batería

widget-rss-name = Noticias
widget-rss-desc = Fuentes de noticias RSS y Atom

widget-recent-files-name = Archivos recientes
widget-recent-files-desc = Archivos abiertos recientemente en Orchid

widget-search-name = Búsqueda universal
widget-search-desc = Buscar archivos, ejecutar comandos, abrir ajustes

widget-media-name = Reproductor multimedia
widget-media-desc = Reproducción actual con controles de transporte

widget-password-name = Contraseñas
widget-password-desc = Acceder a su base de datos de contraseñas

widget-viewer-name = Visor
widget-viewer-desc = Ver imágenes, documentos, archivos fuente y archivos comprimidos

# ---- Weather ----
weather-condition-clear = Despejado
weather-condition-partly-cloudy = Parcialmente nublado
weather-condition-cloudy = Nublado
weather-condition-overcast = Cubierto
weather-condition-fog = Niebla
weather-condition-drizzle = Llovizna
weather-condition-rain = Lluvia
weather-condition-snow = Nieve
weather-condition-sleet = Aguanieve
weather-condition-thunderstorm = Tormenta
weather-condition-hail = Granizo
weather-condition-windy = Ventoso
weather-condition-unknown = Desconocido
weather-day-today = Hoy
weather-day-tomorrow = Mañana
weather-status-fresh = Actualizado
weather-status-stale = Los datos pueden estar desactualizados
weather-status-offline = Sin conexión
weather-status-error = Error al cargar el tiempo
weather-updated-just-now = Actualizado ahora mismo
weather-updated-minutes = Actualizado hace { $m } min
weather-updated-hours = Actualizado hace { $h } h
weather-updated-days = Actualizado hace { $d } d

# ---- Relative time (shared) ----
relative-just-now = ahora mismo
relative-minutes = hace { $m } min
relative-hours = hace { $h } h
relative-days = hace { $d } d

weather-loading = Cargando el tiempo…
weather-feels-like = Sensación { $temp }
weather-humidity-label = Humedad
weather-wind-label = Viento
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
moon-phase-new = Luna nueva
moon-phase-waxing-crescent = Creciente
moon-phase-first-quarter = Cuarto creciente
moon-phase-waxing-gibbous = Gibosa creciente
moon-phase-full = Luna llena
moon-phase-waning-gibbous = Gibosa menguante
moon-phase-last-quarter = Cuarto menguante
moon-phase-waning-crescent = Menguante
moon-illumination = { $pct }% iluminada
moon-age = Edad: { $days } días
moon-distance = Distancia: { $km } km
moon-next-full = Próxima luna llena: { $date }
moon-next-new = Próxima luna nueva: { $date }
moon-moonrise = Salida de la luna: { $time }
moon-moonset = Puesta de la luna: { $time }
moon-sunrise = Amanecer: { $time }
moon-sunset = Atardecer: { $time }
moon-libration = Libration: { $lat }°, { $lon }°
moon-loading = Calculando datos lunares…

# ---- System ----
system-cpu-label = CPU
system-memory-label = Memoria
system-disk-label = Disco { $mount }
system-network-label = Red { $name }
system-battery-label = Batería
system-uptime-label = Tiempo activo
system-battery-charging = Cargando
system-battery-time-remaining = { $time } restantes
system-network-rate = ↑ { $up }/s  ↓ { $down }/s
system-loading = Cargando métricas del sistema…

# ---- RSS ----
rss-no-feeds = No hay fuentes configuradas
rss-loading = Cargando noticias…
rss-fetch-failed = No se pudieron cargar las fuentes. Compruebe la conexión e inténtelo de nuevo.
rss-empty = Aún no hay elementos en las fuentes configuradas.
recent-files-empty = Aún no hay archivos recientes. Abra archivos en el visor o el gestor de archivos para verlos aquí.
rss-error-summary = { $n } de { $total } fuentes no se pudieron actualizar
rss-item-published-minutes = hace { $m } min
rss-item-published-hours = hace { $h } h
rss-item-published-days = hace { $d } d

# ---- Universal Search ----
search-placeholder = Buscar archivos, comandos, ajustes…
search-empty-state = Empiece a escribir para buscar
search-no-results = Sin resultados para «{ $query }»
search-no-results-short = Sin resultados
search-searching = Buscando…
search-source-files = Archivos
search-source-commands = Comandos
search-source-settings = Ajustes

# ---- Command palette ----
command-palette-placeholder = Ejecutar un comando…
command-palette-empty = Todos los comandos

# ---- Registered commands ----
command.widget.create.name = Crear widget
command.widget.create.desc = Añadir un nuevo widget al espacio de trabajo
command.widget.create.arg.type = Id del tipo de widget (p. ej. terminal, weather)

command.widget.close.name = Cerrar widget
command.widget.close.desc = Cerrar una instancia de widget

command.widget.move.name = Mover widget
command.widget.resize.name = Redimensionar widget
command.widget.focus_next.name = Enfocar widget siguiente
command.widget.show_all.name = Mostrar todos los widgets
command.widget.group.dissolve.name = Disolver grupo de widgets

command.workspace.create.name = Crear espacio de trabajo
command.workspace.delete.name = Eliminar espacio de trabajo
command.workspace.switch_to.name = Cambiar a espacio de trabajo
command.workspace.switch_next.name = Espacio de trabajo siguiente
command.workspace.switch_previous.name = Espacio de trabajo anterior

command.terminal.split_horizontal.name = Dividir terminal horizontalmente
command.terminal.split_vertical.name = Dividir terminal verticalmente
command.terminal.tab_new.name = Nueva pestaña de terminal
command.terminal.close.name = Cerrar panel o pestaña de terminal
command.terminal.focus_next_pane.name = Enfocar panel de terminal siguiente
command.terminal.focus_previous_pane.name = Enfocar panel de terminal anterior
command.terminal.tab_next.name = Pestaña de terminal siguiente
command.terminal.tab_previous.name = Pestaña de terminal anterior

# ---- Settings (universal search) ----
settings.section.general = General
settings.section.appearance = Apariencia
settings.section.input = Entrada
settings.section.shortcuts = Atajos
settings.section.locale = Idioma
settings.section.privacy = Privacidad

# ---- Settings panel ----
settings-panel-title = Ajustes
settings-panel-hint = Los valores son de solo lectura por ahora. Edite config.toml directamente; los cambios se recargan automáticamente.
settings-panel-coming-soon = El editor completo de ajustes para esta sección aún no está disponible. Edite config.toml directamente por ahora.
settings-panel-ok = Cerrar

settings-open-in-editor = Abrir en el editor
settings-open-config-file = Abrir config.toml
settings-value-yes = Sí
settings-value-no = No
settings-value-none = Ninguno
settings-value-default = Predeterminado
settings-value-disabled = Desactivado
settings-value-system-default = Predeterminado del sistema
settings-value-hand-left = Izquierda
settings-value-hand-right = Derecha
settings-value-pen-double-tap-none = Ninguno
settings-value-pen-double-tap-switch-tool = Cambiar herramienta
settings-value-pen-double-tap-erase = Borrar
settings-value-sunday = Domingo
settings-value-monday = Lunes

settings-field-auto-update = Actualización automática
settings-field-telemetry = Telemetría
settings-field-open-on-startup = Abrir al iniciar
settings-field-theme = Tema
settings-field-density = Densidad
settings-field-font-family = Familia tipográfica
settings-field-font-scale = Escala de fuente
settings-field-reduce-motion = Reducir animaciones
settings-field-follow-system-theme = Seguir tema del sistema
settings-field-dark-theme = Tema oscuro
settings-field-light-theme = Tema claro
settings-field-primary-hand = Mano dominante
settings-field-mirror-edge-swipes = Invertir deslizamientos en bordes
settings-field-haptic-feedback = Retroalimentación háptica
settings-field-palm-rejection = Rechazo de palma
settings-field-pen-double-tap = Doble toque del lápiz
settings-field-shortcut-overrides = Atajos personalizados
settings-field-leader-key = Tecla líder
settings-field-leader-timeout = Tiempo de espera del líder
settings-field-leader-bindings = Atajos del líder
settings-field-language = Idioma
settings-field-date-format = Formato de fecha
settings-field-time-format = Formato de hora
settings-field-first-day-of-week = Primer día de la semana
settings-field-record-action-history = Registrar historial de acciones
settings-field-history-retention-days = Retención del historial (días)
settings-field-clear-clipboard-seconds = Vaciar portapapeles tras copiar

settings-field-vault-auto-lock = Bloqueo automático de la bóveda (segundos)
command.settings.open.name = Abrir ajustes
command.settings.open.desc = Mostrar el panel de ajustes
command.settings.open_config_file.name = Abrir config
command.settings.open_config_file.desc = Abrir config.toml en el editor predeterminado
command.password.lock.name = Bloquear bóveda de contraseñas
command.password.lock.desc = Borrar la base de contraseñas desbloqueada de la memoria

command.navigation.show_workspace_panel.name = Mostrar panel de espacios de trabajo
command.navigation.show_workspace_panel.desc = Mostrar u ocultar la barra lateral de espacios de trabajo
command.notification.show_center.name = Mostrar centro de notificaciones
command.notification.show_center.desc = Mostrar u ocultar el centro de notificaciones
command.dock.show.name = Mostrar dock
command.dock.show.desc = Mostrar u ocultar el dock de widgets
command.search.show_universal.name = Búsqueda universal
command.search.show_universal.desc = Abrir o enfocar la búsqueda universal
command.onboarding.toggle_hint_mode.name = Alternar modo de sugerencias
command.onboarding.toggle_hint_mode.desc = Mostrar u ocultar sugerencias de gestos en el espacio de trabajo
navigation-workspace-panel-title = Espacios de trabajo
notification-center-title = Notificaciones
notification-center-placeholder = Aún no hay notificaciones.
notification-center-clear = Borrar todo
notification-center-tip-title = Consejo
notification-center-tip-body = Desliza desde el borde derecho o ejecuta «Mostrar centro de notificaciones» para abrir este panel.
# ---- Terminal tab bar ----
terminal-tooltip-split-h = Dividir horizontalmente (Ctrl+Shift+H)
terminal-tooltip-split-v = Dividir verticalmente (Ctrl+Shift+J)
terminal-tooltip-tab-new = Nueva pestaña (Ctrl+Shift+T)

# ---- Media player ----
media-no-session = No hay reproducción
media-loading = Cargando medios…
media-unsupported = Los controles multimedia no están disponibles en esta plataforma
media-play = Reproducir
media-pause = Pausa
media-next = Siguiente
media-previous = Anterior

# ---- Password manager ----
password-locked = La base de datos está bloqueada
password-unlock-label = Contraseña maestra
password-unlock-placeholder = Introducir contraseña maestra
password-unlock-submit = Desbloquear
password-unlock-biometric = Windows Hello
password-unlock-biometric-prompt = Desbloquear bóveda de contraseñas
password-search-placeholder = Buscar entradas…
password-no-entries = Aún no hay entradas
password-copy-password = Copiar contraseña
password-copy-username = Copiar nombre de usuario
password-copy-totp = Copiar TOTP
password-open-url = Abrir URL
password-password-copied = Contraseña copiada (se borra en 30 s)
password-totp-copied = TOTP copiado (se borra en 30 s)
password-totp-remaining = { $s } s


# ==== Viewer widget ====
widget-viewer-name = Visor
widget-viewer-desc = Abrir archivos: imágenes, PDF, texto, archivos comprimidos
viewer-loading = Cargando…
viewer-error = No se puede mostrar este archivo
viewer-unsupported = Tipo de archivo no compatible
viewer-image-fit-screen = Ajustar a pantalla
viewer-image-actual-size = Tamaño real
viewer-image-rotate = Rotar
viewer-image-flip-h = Voltear horizontalmente
viewer-image-flip-v = Voltear verticalmente
viewer-pdf-page-of = Página { $current } de { $total }
viewer-pdf-fit-width = Ajustar al ancho
viewer-pdf-fit-page = Ajustar a página
viewer-pdf-go = Ir
viewer-text-read-only = Solo lectura
viewer-text-editing = Edición
viewer-text-save = Guardar (Ctrl+S)
viewer-text-unsaved-title = Cambios sin guardar
viewer-text-unsaved-body = ¿Guardar los cambios antes de cerrar?
viewer-text-discard = Descartar

viewer-text-dirty-indicator = Cambios sin guardar
viewer-archive-extract-all = Extraer todo
viewer-archive-extract-selected = Extraer selección
viewer-archive-preview-binary = Archivo binario, { $size }

# ==== File manager widget ====
widget-fm-name = Archivos
widget-fm-desc = Explorar, organizar y gestionar archivos
fm-nav-back = Atrás
fm-nav-forward = Adelante
fm-nav-up = Subir
fm-nav-home = Inicio
fm-view-icons = Iconos
fm-view-list = Lista
fm-view-details = Detalles
fm-view-gallery = Galería
fm-sort-name = Nombre
fm-sort-size = Tamaño
fm-sort-modified = Modificado
fm-sort-type = Tipo
fm-action-open = Abrir
fm-action-open-all = Abrir todo
fm-action-open-with = Abrir con…
fm-action-open-default = Abrir con aplicación predeterminada
fm-action-open-in-viewer = Abrir en Orchid Viewer
fm-action-copy = Copiar
fm-action-cut = Cortar
fm-action-paste = Pegar
fm-action-rename = Renombrar
fm-action-delete = Eliminar
fm-action-new-folder = Nueva carpeta
fm-action-new-tab = Nueva pestaña
fm-action-close-tab = Cerrar pestaña
fm-action-select-all = Seleccionar todo
fm-action-deselect-all = Deseleccionar todo
fm-action-star = Destacar
fm-action-unstar = Quitar destacado
fm-action-encrypt = Cifrar
fm-action-reveal = Mostrar temporalmente
fm-action-decrypt = Descifrar
fm-action-add-tag = Añadir etiqueta…
fm-action-remove-tag = Quitar etiqueta
fm-action-color-label = Etiqueta de color
fm-color-red = Rojo
fm-color-orange = Naranja
fm-color-yellow = Amarillo
fm-color-green = Verde
fm-color-blue = Azul
fm-color-purple = Morado
fm-color-gray = Gris
fm-color-none = Sin color
fm-action-properties = Propiedades
fm-action-add-to-managed = Añadir a carpeta gestionada
fm-action-remove-from-managed = Quitar de carpetas gestionadas
fm-action-managed-policy = Política de carpeta gestionada
fm-managed-policy-title = Política de carpeta gestionada
fm-policy-max-size = Tamaño máximo
fm-policy-retention = Retención
fm-policy-excludes = Patrones de exclusión
fm-policy-unlimited = Ilimitado
fm-policy-forever = Conservar siempre
fm-policy-retention-days = { $days } días
fm-policy-none = Ninguno
fm-sidebar-managed-folder-policy = { $name } ({ $count } archivos, { $dedup } ahorrados, política)
fm-sidebar-managed-policy-only = { $name } (política)
fm-rename-title = Renombrar
fm-rename-ok = Aceptar
fm-rename-cancel = Cancelar
fm-dual-pane-on = Doble panel
fm-dual-pane-off = Panel único
fm-show-hidden-on = Mostrar archivos ocultos
fm-show-hidden-off = Ocultar archivos ocultos
fm-click-single-on = Un clic para abrir
fm-click-single-off = Doble clic para abrir
fm-encrypt-title = Cifrar con frase de contraseña
fm-reveal-title = Introducir frase de contraseña para mostrar
fm-decrypt-title = Introducir frase de contraseña para descifrar
fm-info-close = Cerrar
fm-properties-title = Propiedades
fm-tag-add-title = Añadir etiqueta
fm-confirm-delete = ¿Eliminar { $n } elementos?
fm-confirm-delete-permanent = ¿Eliminar permanentemente { $n } elementos?
fm-status-items = { $n } elementos
fm-status-selected = { $n } seleccionados
fm-status-total-size = { $size }
fm-status-bar = { $items } elementos, { $selected } seleccionados
fm-status-managed = { $items } elementos, { $selected } seleccionados · { $tracked } ingeridos, { $dedup } deduplicados
fm-encrypted = Cifrado: { $name }
fm-decrypted = Descifrado: { $name }
fm-managed-added = Añadido a carpeta gestionada
fm-managed-removed = Quitado de carpetas gestionadas
fm-encryption-unavailable = El cifrado no está disponible
fm-passphrase-failed = Frase de contraseña fallida: { $reason }
fm-passphrase-invalid = Frase de contraseña no válida
fm-passphrase-required = Se requiere frase de contraseña
fm-decryption-failed = Error al descifrar
fm-passphrase-encrypt-hint = Elija una frase de contraseña segura. No se puede recuperar si se pierde.
fm-passphrase-decrypt-hint = Introduzca la frase de contraseña usada para cifrar estos archivos.
fm-passphrase-reveal-hint = Los archivos se descifran en una ubicación temporal para su visualización.
fm-passphrase-biometric = Windows Hello
fm-passphrase-biometric-prompt = Desbloquear archivos cifrados
fm-revealed = Mostrado: { $name }
fm-managed-unavailable = Las carpetas gestionadas no están disponibles
fm-managed-no-selection = Seleccione una carpeta para añadir a carpetas gestionadas
fm-not-managed-folder = No es una carpeta gestionada
fm-managed-conflict = Conflicto de carpeta gestionada
fm-sidebar-managed-folder = { $name } ({ $count } archivos, { $dedup } ahorrados)
fm-ingest-failed = Error de ingesta: { $name }
fm-quick-filter-placeholder = Filtrar…
fm-sidebar-favorites = Favoritos
fm-sidebar-categories = Categorías
fm-sidebar-managed = Carpetas gestionadas
fm-network-placeholder = No hay montajes de red configurados. Añada entradas [[file-manager.network-mounts]] en config.toml (SFTP, SMB, WebDAV, FTP vía rclone).
fm-network-no-provider = No hay proveedor de sistema de archivos registrado para esta ubicación de red.
fm-network-rclone-missing = rclone no está instalado o no está en PATH. Configure RCLONE_BIN si es necesario.
fm-network-invalid-mount = Este montaje de red está mal configurado. Compruebe nombre y URI en config.toml.
fm-network-auth-failed = Error de autenticación. Compruebe usuario y contraseña en config.toml.
fm-network-permission-denied = Permiso denegado en esta ubicación de red.
fm-network-connection-failed = No se pudo conectar al host de red. Compruebe la URI y su red.
fm-ingested = Ingerido: { $name }
fm-ingesting = Ingesta: { $name } ({ $count } activos)
fm-ingesting-count = Ingiriendo { $count } archivos…
fm-copying = Copiando: { $name } ({ $percent }%)
fm-moving = Moviendo: { $name } ({ $percent }%)
fm-transfer-failed = Error de transferencia: { $reason }
fm-transfer-already-exists = Ya existe un archivo con ese nombre
fm-transfer-virtual-dest = No se puede copiar o mover a una carpeta virtual
fm-clipboard-copy = { $count } entradas listas para pegar
fm-clipboard-cut = { $count } entradas (cortar) listas para pegar
fm-sidebar-tags = Etiquetas
fm-sidebar-recent = Recientes
fm-sidebar-network = Red
fm-sidebar-network-all = Todos los lugares
fm-category-images = Imágenes
fm-category-documents = Documentos
fm-category-video = Vídeo
fm-category-audio = Audio
fm-category-archives = Archivos comprimidos
fm-virtual-recent = Recientes
fm-virtual-starred = Destacados
fm-virtual-tags = Etiquetas
fm-virtual-recent-empty = Aún no hay archivos recientes. Abra archivos para verlos aquí.
fm-virtual-starred-empty = Aún no hay archivos destacados. Destáquelos desde el menú contextual.
fm-virtual-tags-empty = Aún no hay archivos etiquetados. Añada etiquetas desde el menú contextual.
fm-virtual-category-empty = No se encontraron archivos coincidentes en esta categoría.
fm-virtual-create-denied = No se pueden crear carpetas en una ubicación virtual
fm-empty-folder = Esta carpeta está vacía
fm-error-access = No se puede acceder a esta ubicación


# ==== Startup shell (task 11A) ====
window-title = Orchid
startup-welcome = Bienvenido a Orchid
startup-subtitle = Un entorno informático pensado para el tacto
startup-version-label = Versión { $version }
status-theme = Tema:
status-language = Idioma:
status-density = Densidad:
density-touch = Táctil
density-mouse = Ratón
density-hybrid = Híbrido

# ---- Workspace shell (task 11B) ----
startup-get-started = Empezar
onboarding-back = Atrás
onboarding-next = Siguiente
onboarding-skip = Omitir tour
onboarding-finish = Empezar
onboarding-step-welcome-title = Bienvenido a Orchid
onboarding-step-welcome-body = Orchid es un espacio de trabajo táctil donde gestos, comandos y widgets son tres formas de la misma acción. Este breve recorrido muestra lo esencial.
onboarding-step-workspace-title = Tu espacio de trabajo
onboarding-step-workspace-body = Cambia de espacio de trabajo arriba, organiza widgets en el lienzo y añade nuevos desde el dock abajo.
onboarding-step-palette-title = Paleta de comandos
onboarding-step-palette-body = Pulsa Ctrl+Shift+P para ejecutar cualquier comando. Cada entrada muestra su atajo de teclado para que aprendas sobre la marcha.
onboarding-step-gestures-title = Gestos y sugerencias
onboarding-step-gestures-body = Desliza desde los bordes de la pantalla para paneles y el dock. Pulsa Win+? en cualquier momento para alternar el modo de sugerencias y ver qué está disponible.
onboarding-hint-workspace = Desliza desde el borde izquierdo para espacios de trabajo
onboarding-hint-dock = Desliza hacia arriba desde el borde inferior para el dock
onboarding-hint-gestures = Win+? alterna estas sugerencias
workspace-default-name = Principal
workspace-new = Nuevo espacio de trabajo
workspace-placement-blocked-title = No se puede colocar el widget aquí
workspace-placement-blocked-body = Ese lugar se solapa con otro widget o sale de la cuadrícula. Pruebe una celda libre.
group-tooltip-dissolve = Desagrupar widgets
workspace-unnamed = Espacio de trabajo { $n }
dock-add-label = Añadir widget
catalog-title = Catálogo de widgets
catalog-search-placeholder = Buscar widgets…
dock-widget-terminal = Terminal
dock-widget-weather = Tiempo
dock-widget-moon = Luna
dock-widget-system = Sistema
dock-widget-rss = Noticias
dock-widget-recent-files = Recientes
dock-widget-search = Búsqueda
dock-widget-media = Medios
dock-widget-password = Contraseñas
dock-widget-viewer = Visor
dock-widget-fm = Archivos

viewer-no-file = Ningún archivo abierto
viewer-loading-path = Cargando: { $path }
viewer-error-with-reason = No se puede mostrar este archivo: { $reason }
viewer-pdf-unavailable = La compatibilidad con PDF no está disponible en esta compilación.
viewer-image-heic-unsupported = Las imágenes HEIC aún no son compatibles
viewer-image-raw-unsupported = Las imágenes RAW aún no son compatibles
viewer-archive-select-preview = Seleccione un archivo para previsualizar
viewer-archive-binary-preview = Archivo binario, { $size }

password-select-entry = Seleccionar una entrada
password-label-title = Título
password-label-username = Nombre de usuario
password-label-password = Contraseña
password-label-url = URL
password-label-notes = Notas
password-label-totp = TOTP
password-action-copy = Copiar
password-action-open = Abrir
password-action-lock = Bloquear
password-action-add = Añadir
password-add-title = Nueva entrada
password-add-submit = Guardar
password-add-cancel = Cancelar
password-generate = Generar
password-add-error-title = El título es obligatorio
password-entry-added = Entrada guardada

password-username-copied = Nombre de usuario copiado

moon-age-label = Edad
moon-distance-label = Distancia
moon-next-full-label = Próxima luna llena
moon-next-new-label = Próxima luna nueva
moon-moonrise-label = Salida de la luna
moon-moonset-label = Puesta de la luna
moon-sunrise-label = Amanecer
moon-sunset-label = Atardecer
moon-libration-label = Libration

widget-title-terminal = Terminal
widget-close-tooltip = Cerrar widget
widget-close-confirm = ¿Cerrar { $name }?
action-confirm-yes = Sí
action-confirm-no = No

fm-confirm-title = Confirmar
