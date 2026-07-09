# Orchid French (fr-FR) message catalog.
#
# Consumed by the upcoming `orchid-i18n::LocaleManager`. Until that lands,
# the built-in widgets fall back to the English strings baked into their
# Rust code — the keys below mirror those defaults and will become the
# single source of truth once the bundle loader is wired up.

# ---- Widget registry metadata ----
widget-terminal-name = Terminal
widget-terminal-desc = Shells locaux, WSL ou SSH avec PTY, couleurs ANSI et historique

widget-weather-name = Météo
widget-weather-desc = Conditions actuelles et prévisions sur 3 jours

widget-moon-name = Lune
widget-moon-desc = Phase lunaire actuelle, heures de lever/coucher et données célestes

widget-system-name = Système
widget-system-desc = Indicateurs CPU, mémoire, disque, réseau et batterie

widget-rss-name = Fil d'actualités
widget-rss-desc = Flux d'actualités RSS et Atom

widget-recent-files-name = Fichiers récents
widget-recent-files-desc = Fichiers récemment ouverts dans Orchid

widget-search-name = Recherche universelle
widget-search-desc = Rechercher des fichiers, exécuter des commandes, ouvrir les paramètres

widget-media-name = Lecteur multimédia
widget-media-desc = Lecture en cours avec contrôles de transport

widget-password-name = Mots de passe
widget-password-desc = Accéder à votre base de mots de passe

widget-viewer-name = Visionneuse
widget-viewer-desc = Afficher images, documents, fichiers source et archives

# ---- Weather ----
weather-condition-clear = Dégagé
weather-condition-partly-cloudy = Partiellement nuageux
weather-condition-cloudy = Nuageux
weather-condition-overcast = Couvert
weather-condition-fog = Brouillard
weather-condition-drizzle = Bruine
weather-condition-rain = Pluie
weather-condition-snow = Neige
weather-condition-sleet = Grésil
weather-condition-thunderstorm = Orage
weather-condition-hail = Grêle
weather-condition-windy = Venteux
weather-condition-unknown = Inconnu
weather-day-today = Aujourd'hui
weather-day-tomorrow = Demain
weather-status-fresh = À jour
weather-status-stale = Les données peuvent être obsolètes
weather-status-offline = Hors ligne
weather-status-error = Erreur de chargement de la météo
weather-updated-just-now = Mis à jour à l'instant
weather-updated-minutes = Mis à jour il y a { $m } min
weather-updated-hours = Mis à jour il y a { $h } h
weather-updated-days = Mis à jour il y a { $d } j

# ---- Relative time (shared) ----
relative-just-now = à l'instant
relative-minutes = il y a { $m } min
relative-hours = il y a { $h } h
relative-days = il y a { $d } j

weather-loading = Chargement de la météo…
weather-feels-like = Ressenti { $temp }
weather-humidity-label = Humidité
weather-wind-label = Vent
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
moon-phase-new = Nouvelle lune
moon-phase-waxing-crescent = Premier croissant
moon-phase-first-quarter = Premier quartier
moon-phase-waxing-gibbous = Lune gibbeuse croissante
moon-phase-full = Pleine lune
moon-phase-waning-gibbous = Lune gibbeuse décroissante
moon-phase-last-quarter = Dernier quartier
moon-phase-waning-crescent = Dernier croissant
moon-illumination = { $pct }% illuminée
moon-age = Âge : { $days } jours
moon-distance = Distance : { $km } km
moon-next-full = Prochaine pleine lune : { $date }
moon-next-new = Prochaine nouvelle lune : { $date }
moon-moonrise = Lever de lune : { $time }
moon-moonset = Coucher de lune : { $time }
moon-sunrise = Lever du soleil : { $time }
moon-sunset = Coucher du soleil : { $time }
moon-libration = Libration : { $lat }°, { $lon }°
moon-loading = Calcul des données lunaires…

# ---- System ----
system-cpu-label = CPU
system-memory-label = Mémoire
system-disk-label = Disque { $mount }
system-network-label = Réseau { $name }
system-battery-label = Batterie
system-uptime-label = Disponibilité
system-battery-charging = En charge
system-battery-time-remaining = { $time } restantes
system-network-rate = ↑ { $up }/s  ↓ { $down }/s
system-loading = Chargement des métriques système…

# ---- RSS ----
rss-no-feeds = Aucun flux configuré
rss-loading = Chargement des actualités…
rss-fetch-failed = Impossible de charger les flux. Vérifiez votre connexion et réessayez.
rss-empty = Aucun élément dans les flux configurés pour l'instant.
recent-files-empty = Aucun fichier récent. Ouvrez des fichiers dans la visionneuse ou le gestionnaire de fichiers pour les voir ici.
rss-error-summary = { $n } sur { $total } flux n'ont pas pu être mis à jour
rss-item-published-minutes = il y a { $m } min
rss-item-published-hours = il y a { $h } h
rss-item-published-days = il y a { $d } j

# ---- Universal Search ----
search-placeholder = Rechercher des fichiers, commandes, paramètres…
search-empty-state = Commencez à taper pour rechercher
search-no-results = Aucun résultat pour « { $query } »
search-no-results-short = Aucun résultat
search-searching = Recherche…
search-source-files = Fichiers
search-source-commands = Commandes
search-source-settings = Paramètres

# ---- Command palette ----
command-palette-placeholder = Exécuter une commande…
command-palette-empty = Toutes les commandes

# ---- Registered commands ----
command.widget.create.name = Créer un widget
command.widget.create.desc = Ajouter un nouveau widget à l'espace de travail
command.widget.create.arg.type = Identifiant du type de widget (ex. terminal, weather)

command.widget.close.name = Fermer le widget
command.widget.close.desc = Fermer une instance de widget

command.widget.move.name = Déplacer le widget
command.widget.resize.name = Redimensionner le widget
command.widget.focus_next.name = Widget suivant
command.widget.show_all.name = Afficher tous les widgets
command.widget.group.dissolve.name = Dissoudre le groupe de widgets

command.workspace.create.name = Créer un espace de travail
command.workspace.delete.name = Supprimer l'espace de travail
command.workspace.switch_to.name = Basculer vers l'espace de travail
command.workspace.switch_next.name = Espace de travail suivant
command.workspace.switch_previous.name = Espace de travail précédent

command.terminal.split_horizontal.name = Diviser le terminal horizontalement
command.terminal.split_vertical.name = Diviser le terminal verticalement
command.terminal.tab_new.name = Nouvel onglet terminal
command.terminal.close.name = Fermer le volet ou l'onglet terminal
command.terminal.focus_next_pane.name = Volet terminal suivant
command.terminal.focus_previous_pane.name = Volet terminal précédent
command.terminal.tab_next.name = Onglet terminal suivant
command.terminal.tab_previous.name = Onglet terminal précédent

# ---- Settings (universal search) ----
settings.section.general = Général
settings.section.appearance = Apparence
settings.section.input = Saisie
settings.section.shortcuts = Raccourcis
settings.section.locale = Langue
settings.section.privacy = Confidentialité

# ---- Settings panel ----
settings-panel-title = Paramètres
settings-panel-hint = Les valeurs sont en lecture seule pour l'instant. Modifiez config.toml directement ; les changements se rechargent automatiquement.
settings-panel-coming-soon = L'éditeur complet de paramètres pour cette section n'est pas encore disponible. Modifiez config.toml directement pour l'instant.
settings-panel-ok = Fermer

settings-open-in-editor = Ouvrir dans l'éditeur
settings-open-config-file = Ouvrir config.toml
settings-value-yes = Oui
settings-value-no = Non
settings-value-none = Aucun
settings-value-default = Par défaut
settings-value-disabled = Désactivé
settings-value-system-default = Par défaut du système
settings-value-hand-left = Gauche
settings-value-hand-right = Droite
settings-value-pen-double-tap-none = Aucun
settings-value-pen-double-tap-switch-tool = Changer d'outil
settings-value-pen-double-tap-erase = Effacer
settings-value-sunday = Dimanche
settings-value-monday = Lundi

settings-field-auto-update = Mise à jour automatique
settings-field-telemetry = Télémétrie
settings-field-open-on-startup = Ouvrir au démarrage
settings-field-theme = Thème
settings-field-density = Densité
settings-field-font-family = Police
settings-field-font-scale = Taille de police
settings-field-reduce-motion = Réduire les animations
settings-field-follow-system-theme = Suivre le thème système
settings-field-dark-theme = Thème sombre
settings-field-light-theme = Thème clair
settings-field-primary-hand = Main dominante
settings-field-mirror-edge-swipes = Inverser les balayages sur les bords
settings-field-haptic-feedback = Retour haptique
settings-field-palm-rejection = Rejet de la paume
settings-field-pen-double-tap = Double appui du stylet
settings-field-shortcut-overrides = Raccourcis personnalisés
settings-field-leader-key = Touche leader
settings-field-leader-timeout = Délai leader
settings-field-leader-bindings = Raccourcis leader
settings-field-language = Langue
settings-field-date-format = Format de date
settings-field-time-format = Format d'heure
settings-field-first-day-of-week = Premier jour de la semaine
settings-field-record-action-history = Enregistrer l'historique des actions
settings-field-history-retention-days = Rétention de l'historique (jours)
settings-field-clear-clipboard-seconds = Vider le presse-papiers après copie

settings-field-vault-auto-lock = Verrouillage auto du coffre (secondes)
command.settings.open.name = Ouvrir les paramètres
command.settings.open.desc = Afficher le panneau des paramètres
command.settings.open_config_file.name = Ouvrir la config
command.settings.open_config_file.desc = Ouvrir config.toml dans l'éditeur par défaut
command.password.lock.name = Verrouiller le coffre de mots de passe
command.password.lock.desc = Effacer la base de mots de passe déverrouillée de la mémoire

command.navigation.show_workspace_panel.name = Afficher le panneau d'espaces de travail
command.navigation.show_workspace_panel.desc = Afficher ou masquer la barre latérale des espaces de travail
command.notification.show_center.name = Afficher le centre de notifications
command.notification.show_center.desc = Afficher ou masquer le centre de notifications
command.dock.show.name = Afficher le dock
command.dock.show.desc = Afficher ou masquer le dock de widgets
command.search.show_universal.name = Recherche universelle
command.search.show_universal.desc = Ouvrir ou focaliser la recherche universelle
command.onboarding.toggle_hint_mode.name = Basculer le mode d'indications
command.onboarding.toggle_hint_mode.desc = Afficher ou masquer les indications de gestes sur l'espace de travail
navigation-workspace-panel-title = Espaces de travail
notification-center-title = Notifications
notification-center-placeholder = Aucune notification pour l'instant.
notification-center-clear = Tout effacer
notification-center-tip-title = Astuce
notification-center-tip-body = Balayez depuis le bord droit ou exécutez « Afficher le centre de notifications » pour ouvrir ce panneau.
# ---- Terminal tab bar ----
terminal-tooltip-split-h = Diviser horizontalement (Ctrl+Shift+H)
terminal-tooltip-split-v = Diviser verticalement (Ctrl+Shift+J)
terminal-tooltip-tab-new = Nouvel onglet (Ctrl+Shift+T)

# ---- Media player ----
media-no-session = Aucune lecture en cours
media-loading = Chargement des médias…
media-unsupported = Les contrôles média ne sont pas disponibles sur cette plateforme
media-play = Lecture
media-pause = Pause
media-next = Suivant
media-previous = Précédent

# ---- Password manager ----
password-locked = La base de données est verrouillée
password-unlock-label = Mot de passe principal
password-unlock-placeholder = Saisir le mot de passe principal
password-unlock-submit = Déverrouiller
password-unlock-biometric = Windows Hello
password-unlock-biometric-prompt = Déverrouiller le coffre de mots de passe
password-search-placeholder = Rechercher des entrées…
password-no-entries = Aucune entrée
password-copy-password = Copier le mot de passe
password-copy-username = Copier le nom d'utilisateur
password-copy-totp = Copier le TOTP
password-open-url = Ouvrir l'URL
password-password-copied = Mot de passe copié (effacé dans 30 s)
password-totp-copied = TOTP copié (effacé dans 30 s)
password-totp-remaining = { $s } s


# ==== Viewer widget ====
widget-viewer-name = Visionneuse
widget-viewer-desc = Ouvrir des fichiers : images, PDF, texte, archives
viewer-loading = Chargement…
viewer-error = Impossible d'afficher ce fichier
viewer-unsupported = Type de fichier non pris en charge
viewer-image-fit-screen = Ajuster à l'écran
viewer-image-actual-size = Taille réelle
viewer-image-rotate = Pivoter
viewer-image-flip-h = Retourner horizontalement
viewer-image-flip-v = Retourner verticalement
viewer-pdf-page-of = Page { $current } sur { $total }
viewer-pdf-fit-width = Ajuster à la largeur
viewer-pdf-fit-page = Ajuster à la page
viewer-pdf-go = Aller
viewer-pdf-info = PDF · page { $current } / { $total } · { $width } × { $height } px · { $zoom }%
viewer-image-info = { $width } × { $height } · { $size } · { $format }
viewer-archive-info = { $format }, { $count } entrées
viewer-archive-extracted-selected = Extrait vers { $path }
viewer-archive-extracted-all = { $count } entrées extraites vers { $path }
viewer-archive-nothing-selected = Rien de sélectionné à extraire
viewer-archive-cannot-extract-folder = Impossible d’extraire un dossier
viewer-action-failed = Échec de l’action du visionneur : { $reason }
viewer-text-save-failed = Impossible d’enregistrer le fichier : { $reason }
viewer-text-read-only = Lecture seule
viewer-text-editing = Édition
viewer-text-save = Enregistrer (Ctrl+S)
viewer-text-lines = { $count } lignes
viewer-text-unsaved-title = Modifications non enregistrées
viewer-text-unsaved-body = Enregistrer les modifications avant de fermer ?
viewer-text-discard = Abandonner

viewer-text-dirty-indicator = Modifications non enregistrées
viewer-archive-extract-all = Tout extraire
viewer-archive-extract-selected = Extraire la sélection
viewer-archive-preview-binary = Fichier binaire, { $size }

# ==== File manager widget ====
widget-fm-name = Fichiers
widget-fm-desc = Parcourir, organiser et gérer les fichiers
fm-nav-back = Retour
fm-nav-forward = Suivant
fm-nav-up = Monter
fm-nav-home = Accueil
fm-view-icons = Icônes
fm-view-list = Liste
fm-view-details = Détails
fm-view-gallery = Galerie
fm-sort-name = Nom
fm-sort-size = Taille
fm-sort-modified = Modifié
fm-sort-type = Type
fm-action-open = Ouvrir
fm-action-open-all = Tout ouvrir
fm-action-open-with = Ouvrir avec…
fm-action-open-default = Ouvrir avec l'application par défaut
fm-action-open-in-viewer = Ouvrir dans Orchid Viewer
fm-action-copy = Copier
fm-action-cut = Couper
fm-action-paste = Coller
fm-action-rename = Renommer
fm-action-delete = Supprimer
fm-action-new-folder = Nouveau dossier
fm-action-new-tab = Nouvel onglet
fm-action-close-tab = Fermer l'onglet
fm-action-select-all = Tout sélectionner
fm-action-deselect-all = Tout désélectionner
fm-action-star = Favori
fm-action-unstar = Retirer des favoris
fm-action-encrypt = Chiffrer
fm-action-reveal = Afficher temporairement
fm-action-decrypt = Déchiffrer
fm-action-add-tag = Ajouter une étiquette…
fm-action-remove-tag = Supprimer l'étiquette
fm-action-color-label = Étiquette de couleur
fm-color-red = Rouge
fm-color-orange = Orange
fm-color-yellow = Jaune
fm-color-green = Vert
fm-color-blue = Bleu
fm-color-purple = Violet
fm-color-gray = Gris
fm-color-none = Aucune couleur
fm-action-properties = Propriétés
fm-action-add-to-managed = Ajouter au dossier géré
fm-action-remove-from-managed = Retirer des dossiers gérés
fm-action-managed-policy = Politique de dossier géré
fm-managed-policy-title = Politique de dossier géré
fm-policy-max-size = Taille max.
fm-policy-retention = Rétention
fm-policy-excludes = Motifs d'exclusion
fm-policy-unlimited = Illimité
fm-policy-forever = Conserver indéfiniment
fm-policy-retention-days = { $days } jours
fm-policy-none = Aucun
fm-sidebar-managed-folder-policy = { $name } ({ $count } fichiers, { $dedup } économisés, politique)
fm-sidebar-managed-policy-only = { $name } (politique)
fm-rename-title = Renommer
fm-rename-ok = OK
fm-rename-cancel = Annuler
fm-dual-pane-on = Double volet
fm-dual-pane-off = Volet unique
fm-show-hidden-on = Afficher les fichiers cachés
fm-show-hidden-off = Masquer les fichiers cachés
fm-click-single-on = Un clic pour ouvrir
fm-click-single-off = Double-clic pour ouvrir
fm-encrypt-title = Chiffrer avec une phrase secrète
fm-reveal-title = Saisir la phrase secrète pour afficher
fm-decrypt-title = Saisir la phrase secrète pour déchiffrer
fm-info-close = Fermer
fm-properties-title = Propriétés
fm-tag-add-title = Ajouter une étiquette
fm-confirm-delete = Supprimer { $n } éléments ?
fm-confirm-delete-permanent = Supprimer définitivement { $n } éléments ?
fm-status-items = { $n } éléments
fm-status-selected = { $n } sélectionnés
fm-status-total-size = { $size }
fm-status-bar = { $items } éléments, { $selected } sélectionnés
fm-status-managed = { $items } éléments, { $selected } sélectionnés · { $tracked } ingérés, { $dedup } dédupliqués
fm-encrypted = Chiffré : { $name }
fm-decrypted = Déchiffré : { $name }
fm-managed-added = Ajouté au dossier géré
fm-managed-removed = Retiré des dossiers gérés
fm-encryption-unavailable = Le chiffrement n'est pas disponible
fm-passphrase-failed = Échec de la phrase secrète : { $reason }
fm-passphrase-invalid = Phrase secrète invalide
fm-passphrase-required = La phrase secrète est requise
fm-decryption-failed = Échec du déchiffrement
fm-passphrase-encrypt-hint = Choisissez une phrase secrète forte. Elle ne pourra pas être récupérée en cas de perte.
fm-passphrase-decrypt-hint = Saisissez la phrase secrète utilisée pour chiffrer ces fichiers.
fm-passphrase-reveal-hint = Les fichiers sont déchiffrés vers un emplacement temporaire pour consultation.
fm-passphrase-biometric = Windows Hello
fm-passphrase-biometric-prompt = Déverrouiller les fichiers chiffrés
fm-revealed = Affiché : { $name }
fm-managed-unavailable = Les dossiers gérés ne sont pas disponibles
fm-managed-no-selection = Sélectionnez un dossier à ajouter aux dossiers gérés
fm-not-managed-folder = Ce n'est pas un dossier géré
fm-managed-conflict = Conflit de dossier géré
fm-sidebar-managed-folder = { $name } ({ $count } fichiers, { $dedup } économisés)
fm-ingest-failed = Échec de l'ingestion : { $name }
fm-quick-filter-placeholder = Filtrer…
fm-sidebar-favorites = Favoris
fm-sidebar-categories = Catégories
fm-sidebar-managed = Dossiers gérés
fm-network-placeholder = Aucun montage réseau configuré. Ajoutez des entrées [[file-manager.network-mounts]] dans config.toml (SFTP, SMB, WebDAV, FTP via rclone).
fm-network-no-provider = Aucun fournisseur de système de fichiers n'est enregistré pour cet emplacement réseau.
fm-network-rclone-missing = rclone n'est pas installé ou absent du PATH. Définissez RCLONE_BIN si nécessaire.
fm-network-invalid-mount = Ce montage réseau est mal configuré. Vérifiez le nom et l'URI dans config.toml.
fm-network-auth-failed = Échec de l'authentification. Vérifiez le nom d'utilisateur et le mot de passe dans config.toml.
fm-network-permission-denied = Permission refusée sur cet emplacement réseau.
fm-network-connection-failed = Impossible de se connecter à l'hôte réseau. Vérifiez l'URI et votre réseau.
fm-ingested = Ingéré : { $name }
fm-ingesting = Ingestion : { $name } ({ $count } actifs)
fm-ingesting-count = Ingestion de { $count } fichiers…
fm-copying = Copie : { $name } ({ $percent }%)
fm-moving = Déplacement : { $name } ({ $percent }%)
fm-transfer-failed = Échec du transfert : { $reason }
fm-transfer-already-exists = Un fichier portant ce nom existe déjà
fm-transfer-virtual-dest = Impossible de copier ou déplacer vers un dossier virtuel
fm-clipboard-copy = { $count } entrées prêtes à coller
fm-clipboard-cut = { $count } entrées (couper) prêtes à coller
fm-sidebar-tags = Étiquettes
fm-sidebar-recent = Récents
fm-sidebar-network = Réseau
fm-sidebar-network-all = Tous les emplacements
fm-category-images = Images
fm-category-documents = Documents
fm-category-video = Vidéo
fm-category-audio = Audio
fm-category-archives = Archives
fm-virtual-recent = Récents
fm-virtual-starred = Favoris
fm-virtual-tags = Étiquettes
fm-virtual-recent-empty = Aucun fichier récent. Ouvrez des fichiers pour les voir ici.
fm-virtual-starred-empty = Aucun favori. Ajoutez des favoris depuis le menu contextuel.
fm-virtual-tags-empty = Aucun fichier étiqueté. Ajoutez des étiquettes depuis le menu contextuel.
fm-virtual-category-empty = Aucun fichier correspondant dans cette catégorie.
fm-virtual-create-denied = Impossible de créer des dossiers dans un emplacement virtuel
fm-empty-folder = Ce dossier est vide
fm-error-access = Impossible d'accéder à cet emplacement


# ==== Startup shell (task 11A) ====
window-title = Orchid
startup-welcome = Bienvenue dans Orchid
startup-subtitle = Un environnement informatique pensé pour le tactile
startup-version-label = Version { $version }
status-theme = Thème :
status-language = Langue :
status-density = Densité :
density-touch = Tactile
density-mouse = Souris
density-hybrid = Hybride

# ---- Workspace shell (task 11B) ----
startup-get-started = Commencer
onboarding-back = Retour
onboarding-next = Suivant
onboarding-skip = Ignorer la visite
onboarding-finish = Commencer
onboarding-step-welcome-title = Bienvenue dans Orchid
onboarding-step-welcome-body = Orchid est un espace de travail tactile où gestes, commandes et widgets sont trois formes de la même action. Cette courte visite présente l'essentiel.
onboarding-step-workspace-title = Votre espace de travail
onboarding-step-workspace-body = Changez d'espace de travail en haut, disposez les widgets sur le canevas et ajoutez-en depuis le dock en bas.
onboarding-step-palette-title = Palette de commandes
onboarding-step-palette-body = Appuyez sur Ctrl+Shift+P pour exécuter une commande. Chaque entrée affiche son raccourci clavier pour apprendre en pratiquant.
onboarding-step-gestures-title = Gestes et indications
onboarding-step-gestures-body = Balayez depuis les bords de l'écran pour les panneaux et le dock. Appuyez sur Win+? à tout moment pour basculer le mode d'indications et voir ce qui est disponible.
onboarding-hint-workspace = Balayez depuis le bord gauche pour les espaces de travail
onboarding-hint-dock = Balayez vers le haut depuis le bord inférieur pour le dock
onboarding-hint-gestures = Win+? bascule ces indications
workspace-default-name = Principal
workspace-new = Nouvel espace de travail
workspace-placement-blocked-title = Impossible de placer le widget ici
workspace-placement-blocked-body = Cet emplacement chevauche un autre widget ou sort de la grille. Essayez une cellule libre.
group-tooltip-dissolve = Dissocier les widgets
group-tooltip-move-left = Déplacer l'onglet à gauche
group-tooltip-move-right = Déplacer l'onglet à droite
group-tooltip-close-tab = Retirer du groupe
group-hint-alt-detach = Alt+glisser pour détacher du groupe
workspace-unnamed = Espace de travail { $n }
dock-add-label = Ajouter un widget
catalog-title = Catalogue de widgets
catalog-search-placeholder = Rechercher des widgets…
dock-widget-terminal = Terminal
dock-widget-weather = Météo
dock-widget-moon = Lune
dock-widget-system = Système
dock-widget-rss = Actualités
dock-widget-recent-files = Récents
dock-widget-search = Recherche
dock-widget-media = Médias
dock-widget-password = Mots de passe
dock-widget-viewer = Visionneuse
dock-widget-fm = Fichiers

viewer-no-file = Aucun fichier ouvert
viewer-loading-path = Chargement : { $path }
viewer-error-with-reason = Impossible d'afficher ce fichier : { $reason }
viewer-pdf-unavailable = La prise en charge PDF n'est pas disponible dans cette version.
viewer-image-heic-unsupported = Les images HEIC ne sont pas encore prises en charge
viewer-image-raw-unsupported = Les images RAW ne sont pas encore prises en charge
viewer-archive-select-preview = Sélectionnez un fichier à prévisualiser
viewer-archive-binary-preview = Fichier binaire, { $size }

password-select-entry = Sélectionner une entrée
password-label-title = Titre
password-label-username = Nom d'utilisateur
password-label-password = Mot de passe
password-label-url = URL
password-label-notes = Notes
password-label-totp = TOTP
password-action-copy = Copier
password-action-open = Ouvrir
password-action-lock = Verrouiller
password-action-add = Ajouter
password-add-title = Nouvelle entrée
password-add-submit = Enregistrer
password-add-cancel = Annuler
password-generate = Générer
password-add-error-title = Le titre est requis
password-entry-added = Entrée enregistrée

password-username-copied = Nom d'utilisateur copié

moon-age-label = Âge
moon-distance-label = Distance
moon-next-full-label = Prochaine pleine lune
moon-next-new-label = Prochaine nouvelle lune
moon-moonrise-label = Lever de lune
moon-moonset-label = Coucher de lune
moon-sunrise-label = Lever du soleil
moon-sunset-label = Coucher du soleil
moon-libration-label = Libration

widget-title-terminal = Terminal
widget-close-tooltip = Fermer le widget
widget-close-confirm = Fermer { $name } ?
action-confirm-yes = Oui
action-confirm-no = Non

fm-confirm-title = Confirmer
