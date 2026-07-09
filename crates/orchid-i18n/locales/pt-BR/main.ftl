# Orchid Brazilian Portuguese (pt-BR) message catalog.
#
# Consumed by the upcoming `orchid-i18n::LocaleManager`. Until that lands,
# the built-in widgets fall back to the English strings baked into their
# Rust code — the keys below mirror those defaults and will become the
# single source of truth once the bundle loader is wired up.

# ---- Widget registry metadata ----
widget-terminal-name = Terminal
widget-terminal-desc = Shells locais, WSL ou SSH com PTY, cores ANSI e histórico

widget-weather-name = Clima
widget-weather-desc = Condições atuais e previsão de 3 dias

widget-moon-name = Lua
widget-moon-desc = Fase lunar atual, horários de nascer/pôr e dados celestes

widget-system-name = Sistema
widget-system-desc = Indicadores de CPU, memória, disco, rede e bateria

widget-rss-name = Notícias
widget-rss-desc = Feeds de notícias RSS e Atom

widget-recent-files-name = Arquivos recentes
widget-recent-files-desc = Arquivos abertos recentemente no Orchid

widget-search-name = Busca universal
widget-search-desc = Buscar arquivos, executar comandos, abrir configurações

widget-media-name = Reprodutor de mídia
widget-media-desc = Reprodução atual com controles de transporte

widget-password-name = Senhas
widget-password-desc = Acessar seu banco de dados de senhas

widget-viewer-name = Visualizador
widget-viewer-desc = Ver imagens, documentos, arquivos de código e compactados

# ---- Weather ----
weather-condition-clear = Limpo
weather-condition-partly-cloudy = Parcialmente nublado
weather-condition-cloudy = Nublado
weather-condition-overcast = Encoberto
weather-condition-fog = Neblina
weather-condition-drizzle = Garoa
weather-condition-rain = Chuva
weather-condition-snow = Neve
weather-condition-sleet = Granizo
weather-condition-thunderstorm = Tempestade
weather-condition-hail = Granizo
weather-condition-windy = Ventoso
weather-condition-unknown = Desconhecido
weather-day-today = Hoje
weather-day-tomorrow = Amanhã
weather-status-fresh = Atualizado
weather-status-stale = Os dados podem estar desatualizados
weather-status-offline = Offline
weather-status-error = Erro ao carregar o clima
weather-updated-just-now = Atualizado agora
weather-updated-minutes = Atualizado há { $m } min
weather-updated-hours = Atualizado há { $h } h
weather-updated-days = Atualizado há { $d } d

# ---- Relative time (shared) ----
relative-just-now = agora
relative-minutes = há { $m } min
relative-hours = há { $h } h
relative-days = há { $d } d

weather-loading = Carregando clima…
weather-feels-like = Sensação { $temp }
weather-humidity-label = Umidade
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
moon-phase-new = Lua nova
moon-phase-waxing-crescent = Crescente
moon-phase-first-quarter = Quarto crescente
moon-phase-waxing-gibbous = Gibosa crescente
moon-phase-full = Lua cheia
moon-phase-waning-gibbous = Gibosa minguante
moon-phase-last-quarter = Quarto minguante
moon-phase-waning-crescent = Minguante
moon-illumination = { $pct }% iluminada
moon-age = Idade: { $days } dias
moon-distance = Distância: { $km } km
moon-next-full = Próxima lua cheia: { $date }
moon-next-new = Próxima lua nova: { $date }
moon-moonrise = Nascer da lua: { $time }
moon-moonset = Pôr da lua: { $time }
moon-sunrise = Nascer do sol: { $time }
moon-sunset = Pôr do sol: { $time }
moon-libration = Libration: { $lat }°, { $lon }°
moon-loading = Calculando dados lunares…

# ---- System ----
system-cpu-label = CPU
system-memory-label = Memória
system-disk-label = Disco { $mount }
system-network-label = Rede { $name }
system-battery-label = Bateria
system-uptime-label = Tempo ativo
system-battery-charging = Carregando
system-battery-time-remaining = { $time } restantes
system-network-rate = ↑ { $up }/s  ↓ { $down }/s
system-loading = Carregando métricas do sistema…

# ---- RSS ----
rss-no-feeds = Nenhum feed configurado
rss-loading = Carregando notícias…
rss-fetch-failed = Não foi possível carregar os feeds. Verifique a conexão e tente novamente.
rss-empty = Ainda não há itens nos feeds configurados.
recent-files-empty = Nenhum arquivo recente. Abra arquivos no visualizador ou gerenciador de arquivos para vê-los aqui.
rss-error-summary = { $n } de { $total } feeds falharam ao atualizar
rss-item-published-minutes = há { $m } min
rss-item-published-hours = há { $h } h
rss-item-published-days = há { $d } d

# ---- Universal Search ----
search-placeholder = Buscar arquivos, comandos, configurações…
search-empty-state = Comece a digitar para buscar
search-no-results = Nenhum resultado para "{ $query }"
search-no-results-short = Nenhum resultado
search-searching = Buscando…
search-source-files = Arquivos
search-source-commands = Comandos
search-source-settings = Configurações

# ---- Command palette ----
command-palette-placeholder = Executar um comando…
command-palette-empty = Todos os comandos

# ---- Registered commands ----
command.widget.create.name = Criar widget
command.widget.create.desc = Adicionar um novo widget ao espaço de trabalho
command.widget.create.arg.type = Id do tipo de widget (ex.: terminal, weather)

command.widget.close.name = Fechar widget
command.widget.close.desc = Fechar uma instância de widget

command.widget.move.name = Mover widget
command.widget.resize.name = Redimensionar widget
command.widget.focus_next.name = Focar próximo widget
command.widget.show_all.name = Mostrar todos os widgets
command.widget.group.dissolve.name = Dissolver grupo de widgets

command.workspace.create.name = Criar espaço de trabalho
command.workspace.delete.name = Excluir espaço de trabalho
command.workspace.switch_to.name = Mudar para espaço de trabalho
command.workspace.switch_next.name = Próximo espaço de trabalho
command.workspace.switch_previous.name = Espaço de trabalho anterior

command.terminal.split_horizontal.name = Dividir terminal horizontalmente
command.terminal.split_vertical.name = Dividir terminal verticalmente
command.terminal.tab_new.name = Nova aba do terminal
command.terminal.close.name = Fechar painel ou aba do terminal
command.terminal.focus_next_pane.name = Focar próximo painel do terminal
command.terminal.focus_previous_pane.name = Focar painel anterior do terminal
command.terminal.tab_next.name = Próxima aba do terminal
command.terminal.tab_previous.name = Aba anterior do terminal

# ---- Settings (universal search) ----
settings.section.general = Geral
settings.section.appearance = Aparência
settings.section.input = Entrada
settings.section.shortcuts = Atalhos
settings.section.locale = Idioma
settings.section.privacy = Privacidade

# ---- Settings panel ----
settings-panel-title = Configurações
settings-panel-hint = Os valores são somente leitura por enquanto. Edite config.toml diretamente; as alterações recarregam automaticamente.
settings-panel-coming-soon = O editor completo de configurações para esta seção ainda não está disponível. Edite config.toml diretamente por enquanto.
settings-panel-ok = Fechar

settings-open-in-editor = Abrir no editor
settings-open-config-file = Abrir config.toml
settings-value-yes = Sim
settings-value-no = Não
settings-value-none = Nenhum
settings-value-default = Padrão
settings-value-disabled = Desativado
settings-value-system-default = Padrão do sistema
settings-value-hand-left = Esquerda
settings-value-hand-right = Direita
settings-value-pen-double-tap-none = Nenhum
settings-value-pen-double-tap-switch-tool = Trocar ferramenta
settings-value-pen-double-tap-erase = Apagar
settings-value-sunday = Domingo
settings-value-monday = Segunda-feira

settings-field-auto-update = Atualização automática
settings-field-telemetry = Telemetria
settings-field-open-on-startup = Abrir ao iniciar
settings-field-theme = Tema
settings-field-density = Densidade
settings-field-font-family = Família de fontes
settings-field-font-scale = Escala da fonte
settings-field-reduce-motion = Reduzir animações
settings-field-follow-system-theme = Seguir tema do sistema
settings-field-dark-theme = Tema escuro
settings-field-light-theme = Tema claro
settings-field-primary-hand = Mão dominante
settings-field-mirror-edge-swipes = Espelhar gestos nas bordas
settings-field-haptic-feedback = Feedback háptico
settings-field-palm-rejection = Rejeição de palma
settings-field-pen-double-tap = Toque duplo da caneta
settings-field-shortcut-overrides = Atalhos personalizados
settings-field-leader-key = Tecla líder
settings-field-leader-timeout = Tempo limite do líder
settings-field-leader-bindings = Atalhos do líder
settings-field-language = Idioma
settings-field-date-format = Formato de data
settings-field-time-format = Formato de hora
settings-field-first-day-of-week = Primeiro dia da semana
settings-field-record-action-history = Registrar histórico de ações
settings-field-history-retention-days = Retenção do histórico (dias)
settings-field-clear-clipboard-seconds = Limpar área de transferência após copiar

settings-field-vault-auto-lock = Bloqueio automático do cofre (segundos)
command.settings.open.name = Abrir configurações
command.settings.open.desc = Mostrar o painel de configurações
command.settings.open_config_file.name = Abrir config
command.settings.open_config_file.desc = Abrir config.toml no editor padrão
command.password.lock.name = Bloquear cofre de senhas
command.password.lock.desc = Remover o banco de senhas desbloqueado da memória

command.navigation.show_workspace_panel.name = Mostrar painel de áreas de trabalho
command.navigation.show_workspace_panel.desc = Mostrar ou ocultar a barra lateral de áreas de trabalho
command.notification.show_center.name = Mostrar central de notificações
command.notification.show_center.desc = Mostrar ou ocultar a central de notificações
command.dock.show.name = Mostrar dock
command.dock.show.desc = Mostrar ou ocultar o dock de widgets
command.search.show_universal.name = Busca universal
command.search.show_universal.desc = Abrir ou focar a busca universal
command.onboarding.toggle_hint_mode.name = Alternar modo de dicas
command.onboarding.toggle_hint_mode.desc = Mostrar ou ocultar dicas de gestos na área de trabalho
navigation-workspace-panel-title = Áreas de trabalho
notification-center-title = Notificações
notification-center-placeholder = Nenhuma notificação ainda.
notification-center-clear = Limpar tudo
notification-center-tip-title = Dica
notification-center-tip-body = Deslize da borda direita ou execute «Mostrar central de notificações» para abrir este painel.
# ---- Terminal tab bar ----
terminal-tooltip-split-h = Dividir horizontalmente (Ctrl+Shift+H)
terminal-tooltip-split-v = Dividir verticalmente (Ctrl+Shift+J)
terminal-tooltip-tab-new = Nova aba (Ctrl+Shift+T)

# ---- Media player ----
media-no-session = Nenhuma mídia em reprodução
media-loading = Carregando mídia…
media-unsupported = Controles de mídia não estão disponíveis nesta plataforma
media-play = Reproduzir
media-pause = Pausar
media-next = Próximo
media-previous = Anterior

# ---- Password manager ----
password-locked = O banco de dados está bloqueado
password-unlock-label = Senha mestra
password-unlock-placeholder = Digite a senha mestra
password-unlock-submit = Desbloquear
password-unlock-biometric = Windows Hello
password-unlock-biometric-prompt = Desbloquear cofre de senhas
password-search-placeholder = Buscar entradas…
password-no-entries = Nenhuma entrada ainda
password-copy-password = Copiar senha
password-copy-username = Copiar nome de usuário
password-copy-totp = Copiar TOTP
password-open-url = Abrir URL
password-password-copied = Senha copiada (limpa em 30 s)
password-totp-copied = TOTP copiado (limpo em 30 s)
password-totp-remaining = { $s } s


# ==== Viewer widget ====
widget-viewer-name = Visualizador
widget-viewer-desc = Abrir arquivos: imagens, PDF, texto, compactados
viewer-loading = Carregando…
viewer-error = Não é possível exibir este arquivo
viewer-unsupported = Tipo de arquivo não suportado
viewer-image-fit-screen = Ajustar à tela
viewer-image-actual-size = Tamanho real
viewer-image-rotate = Girar
viewer-image-flip-h = Inverter horizontalmente
viewer-image-flip-v = Inverter verticalmente
viewer-pdf-page-of = Página { $current } de { $total }
viewer-pdf-fit-width = Ajustar à largura
viewer-pdf-fit-page = Ajustar à página
viewer-pdf-go = Ir
viewer-pdf-info = PDF · pág. { $current } / { $total } · { $width } × { $height } px · { $zoom }%
viewer-image-info = { $width } × { $height } · { $size } · { $format }
viewer-archive-info = { $format }, { $count } entradas
viewer-archive-extracted-selected = Extraído para { $path }
viewer-archive-extracted-all = Extraídas { $count } entradas para { $path }
viewer-text-read-only = Somente leitura
viewer-text-editing = Editando
viewer-text-save = Salvar (Ctrl+S)
viewer-text-lines = { $count } linhas
viewer-text-unsaved-title = Alterações não salvas
viewer-text-unsaved-body = Salvar as alterações antes de fechar?
viewer-text-discard = Descartar

viewer-text-dirty-indicator = Alterações não salvas
viewer-archive-extract-all = Extrair tudo
viewer-archive-extract-selected = Extrair selecionados
viewer-archive-preview-binary = Arquivo binário, { $size }

# ==== File manager widget ====
widget-fm-name = Arquivos
widget-fm-desc = Navegar, organizar e gerenciar arquivos
fm-nav-back = Voltar
fm-nav-forward = Avançar
fm-nav-up = Subir
fm-nav-home = Início
fm-view-icons = Ícones
fm-view-list = Lista
fm-view-details = Detalhes
fm-view-gallery = Galeria
fm-sort-name = Nome
fm-sort-size = Tamanho
fm-sort-modified = Modificado
fm-sort-type = Tipo
fm-action-open = Abrir
fm-action-open-all = Abrir tudo
fm-action-open-with = Abrir com…
fm-action-open-default = Abrir com app padrão
fm-action-open-in-viewer = Abrir no Orchid Viewer
fm-action-copy = Copiar
fm-action-cut = Recortar
fm-action-paste = Colar
fm-action-rename = Renomear
fm-action-delete = Excluir
fm-action-new-folder = Nova pasta
fm-action-new-tab = Nova aba
fm-action-close-tab = Fechar aba
fm-action-select-all = Selecionar tudo
fm-action-deselect-all = Desmarcar tudo
fm-action-star = Favoritar
fm-action-unstar = Remover favorito
fm-action-encrypt = Criptografar
fm-action-reveal = Revelar temporariamente
fm-action-decrypt = Descriptografar
fm-action-add-tag = Adicionar tag…
fm-action-remove-tag = Remover tag
fm-action-color-label = Etiqueta de cor
fm-color-red = Vermelho
fm-color-orange = Laranja
fm-color-yellow = Amarelo
fm-color-green = Verde
fm-color-blue = Azul
fm-color-purple = Roxo
fm-color-gray = Cinza
fm-color-none = Sem cor
fm-action-properties = Propriedades
fm-action-add-to-managed = Adicionar à pasta gerenciada
fm-action-remove-from-managed = Remover das pastas gerenciadas
fm-action-managed-policy = Política de pasta gerenciada
fm-managed-policy-title = Política de pasta gerenciada
fm-policy-max-size = Tamanho máximo
fm-policy-retention = Retenção
fm-policy-excludes = Padrões de exclusão
fm-policy-unlimited = Ilimitado
fm-policy-forever = Manter para sempre
fm-policy-retention-days = { $days } dias
fm-policy-none = Nenhum
fm-sidebar-managed-folder-policy = { $name } ({ $count } arquivos, { $dedup } economizados, política)
fm-sidebar-managed-policy-only = { $name } (política)
fm-rename-title = Renomear
fm-rename-ok = OK
fm-rename-cancel = Cancelar
fm-dual-pane-on = Dois painéis
fm-dual-pane-off = Painel único
fm-show-hidden-on = Mostrar arquivos ocultos
fm-show-hidden-off = Ocultar arquivos ocultos
fm-click-single-on = Um clique para abrir
fm-click-single-off = Dois cliques para abrir
fm-encrypt-title = Criptografar com senha
fm-reveal-title = Digite a senha para revelar
fm-decrypt-title = Digite a senha para descriptografar
fm-info-close = Fechar
fm-properties-title = Propriedades
fm-tag-add-title = Adicionar tag
fm-confirm-delete = Excluir { $n } itens?
fm-confirm-delete-permanent = Excluir permanentemente { $n } itens?
fm-status-items = { $n } itens
fm-status-selected = { $n } selecionados
fm-status-total-size = { $size }
fm-status-bar = { $items } itens, { $selected } selecionados
fm-status-managed = { $items } itens, { $selected } selecionados · { $tracked } ingeridos, { $dedup } deduplicados
fm-encrypted = Criptografado: { $name }
fm-decrypted = Descriptografado: { $name }
fm-managed-added = Adicionado à pasta gerenciada
fm-managed-removed = Removido das pastas gerenciadas
fm-encryption-unavailable = A criptografia não está disponível
fm-passphrase-failed = Senha falhou: { $reason }
fm-passphrase-invalid = Senha inválida
fm-passphrase-required = Senha é obrigatória
fm-decryption-failed = Falha na descriptografia
fm-passphrase-encrypt-hint = Escolha uma senha forte. Ela não pode ser recuperada se for perdida.
fm-passphrase-decrypt-hint = Digite a senha usada para criptografar estes arquivos.
fm-passphrase-reveal-hint = Os arquivos são descriptografados em um local temporário para visualização.
fm-passphrase-biometric = Windows Hello
fm-passphrase-biometric-prompt = Desbloquear arquivos criptografados
fm-revealed = Revelado: { $name }
fm-managed-unavailable = Pastas gerenciadas não estão disponíveis
fm-managed-no-selection = Selecione uma pasta para adicionar às pastas gerenciadas
fm-not-managed-folder = Não é uma pasta gerenciada
fm-managed-conflict = Conflito de pasta gerenciada
fm-sidebar-managed-folder = { $name } ({ $count } arquivos, { $dedup } economizados)
fm-ingest-failed = Falha na ingestão: { $name }
fm-quick-filter-placeholder = Filtrar…
fm-sidebar-favorites = Favoritos
fm-sidebar-categories = Categorias
fm-sidebar-managed = Pastas gerenciadas
fm-network-placeholder = Nenhuma montagem de rede configurada. Adicione entradas [[file-manager.network-mounts]] em config.toml (SFTP, SMB, WebDAV, FTP via rclone).
fm-network-no-provider = Nenhum provedor de sistema de arquivos registrado para este local de rede.
fm-network-rclone-missing = rclone não está instalado ou não está no PATH. Defina RCLONE_BIN se necessário.
fm-network-invalid-mount = Esta montagem de rede está mal configurada. Verifique nome e URI em config.toml.
fm-network-auth-failed = Falha na autenticação. Verifique usuário e senha em config.toml.
fm-network-permission-denied = Permissão negada neste local de rede.
fm-network-connection-failed = Não foi possível conectar ao host de rede. Verifique a URI e sua rede.
fm-ingested = Ingerido: { $name }
fm-ingesting = Ingestão: { $name } ({ $count } ativos)
fm-ingesting-count = Ingerindo { $count } arquivos…
fm-copying = Copiando: { $name } ({ $percent }%)
fm-moving = Movendo: { $name } ({ $percent }%)
fm-transfer-failed = Falha na transferência: { $reason }
fm-transfer-already-exists = Já existe um arquivo com esse nome
fm-transfer-virtual-dest = Não é possível copiar ou mover para uma pasta virtual
fm-clipboard-copy = { $count } entradas prontas para colar
fm-clipboard-cut = { $count } entradas (recortar) prontas para colar
fm-sidebar-tags = Tags
fm-sidebar-recent = Recentes
fm-sidebar-network = Rede
fm-sidebar-network-all = Todos os locais
fm-category-images = Imagens
fm-category-documents = Documentos
fm-category-video = Vídeo
fm-category-audio = Áudio
fm-category-archives = Arquivos compactados
fm-virtual-recent = Recentes
fm-virtual-starred = Favoritos
fm-virtual-tags = Tags
fm-virtual-recent-empty = Nenhum arquivo recente. Abra arquivos para vê-los aqui.
fm-virtual-starred-empty = Nenhum favorito. Favorite itens no menu de contexto.
fm-virtual-tags-empty = Nenhum arquivo com tag. Adicione tags no menu de contexto.
fm-virtual-category-empty = Nenhum arquivo correspondente nesta categoria.
fm-virtual-create-denied = Não é possível criar pastas em um local virtual
fm-empty-folder = Esta pasta está vazia
fm-error-access = Não é possível acessar este local


# ==== Startup shell (task 11A) ====
window-title = Orchid
startup-welcome = Bem-vindo ao Orchid
startup-subtitle = Um ambiente de computação pensado para o toque
startup-version-label = Versão { $version }
status-theme = Tema:
status-language = Idioma:
status-density = Densidade:
density-touch = Toque
density-mouse = Mouse
density-hybrid = Híbrido

# ---- Workspace shell (task 11B) ----
startup-get-started = Começar
onboarding-back = Voltar
onboarding-next = Avançar
onboarding-skip = Pular tour
onboarding-finish = Começar
onboarding-step-welcome-title = Bem-vindo ao Orchid
onboarding-step-welcome-body = Orchid é um ambiente touch-first onde gestos, comandos e widgets são três formas da mesma ação. Este tour rápido mostra o essencial.
onboarding-step-workspace-title = Sua área de trabalho
onboarding-step-workspace-body = Alterne áreas de trabalho no topo, organize widgets na tela e adicione novos pelo dock na parte inferior.
onboarding-step-palette-title = Paleta de comandos
onboarding-step-palette-body = Pressione Ctrl+Shift+P para executar qualquer comando. Cada entrada mostra seu atalho de teclado para você aprender no caminho.
onboarding-step-gestures-title = Gestos e dicas
onboarding-step-gestures-body = Deslize das bordas da tela para painéis e o dock. Pressione Win+? a qualquer momento para alternar o modo de dicas e ver o que está disponível.
onboarding-hint-workspace = Deslize da borda esquerda para áreas de trabalho
onboarding-hint-dock = Deslize de baixo para cima da borda inferior para o dock
onboarding-hint-gestures = Win+? alterna estas dicas
workspace-default-name = Principal
workspace-new = Novo espaço de trabalho
workspace-placement-blocked-title = Não é possível colocar o widget aqui
workspace-placement-blocked-body = Esse local sobrepõe outro widget ou sai da grade. Tente uma célula livre.
group-tooltip-dissolve = Desagrupar widgets
group-tooltip-move-left = Mover aba para a esquerda
group-tooltip-move-right = Mover aba para a direita
group-tooltip-close-tab = Remover do grupo
group-hint-alt-detach = Alt+arrastar para separar do grupo
workspace-unnamed = Espaço de trabalho { $n }
dock-add-label = Adicionar widget
catalog-title = Catálogo de widgets
catalog-search-placeholder = Buscar widgets…
dock-widget-terminal = Terminal
dock-widget-weather = Clima
dock-widget-moon = Lua
dock-widget-system = Sistema
dock-widget-rss = Notícias
dock-widget-recent-files = Recentes
dock-widget-search = Busca
dock-widget-media = Mídia
dock-widget-password = Senhas
dock-widget-viewer = Visualizador
dock-widget-fm = Arquivos

viewer-no-file = Nenhum arquivo aberto
viewer-loading-path = Carregando: { $path }
viewer-error-with-reason = Não é possível exibir este arquivo: { $reason }
viewer-pdf-unavailable = O suporte a PDF não está disponível nesta compilação.
viewer-image-heic-unsupported = Imagens HEIC ainda não são suportadas
viewer-image-raw-unsupported = Imagens RAW ainda não são suportadas
viewer-archive-select-preview = Selecione um arquivo para visualizar
viewer-archive-binary-preview = Arquivo binário, { $size }

password-select-entry = Selecionar uma entrada
password-label-title = Título
password-label-username = Nome de usuário
password-label-password = Senha
password-label-url = URL
password-label-notes = Notas
password-label-totp = TOTP
password-action-copy = Copiar
password-action-open = Abrir
password-action-lock = Bloquear
password-action-add = Adicionar
password-add-title = Nova entrada
password-add-submit = Salvar
password-add-cancel = Cancelar
password-generate = Gerar
password-add-error-title = O título é obrigatório
password-entry-added = Entrada salva

password-username-copied = Nome de usuário copiado

moon-age-label = Idade
moon-distance-label = Distância
moon-next-full-label = Próxima lua cheia
moon-next-new-label = Próxima lua nova
moon-moonrise-label = Nascer da lua
moon-moonset-label = Pôr da lua
moon-sunrise-label = Nascer do sol
moon-sunset-label = Pôr do sol
moon-libration-label = Libration

widget-title-terminal = Terminal
widget-close-tooltip = Fechar widget
widget-close-confirm = Fechar { $name }?
action-confirm-yes = Sim
action-confirm-no = Não

fm-confirm-title = Confirmar
