# Orchid Korean (ko-KR) message catalog.
#
# Consumed by the upcoming `orchid-i18n::LocaleManager`. Until that lands,
# the built-in widgets fall back to the English strings baked into their
# Rust code — the keys below mirror those defaults and will become the
# single source of truth once the bundle loader is wired up.

# ---- Widget registry metadata ----
widget-terminal-name = 터미널
widget-terminal-desc = PTY, ANSI 색상, 스크롤백을 지원하는 로컬, WSL 또는 SSH 셸

widget-weather-name = 날씨
widget-weather-desc = 현재 상태 및 3일 예보

widget-moon-name = 달
widget-moon-desc = 현재 월상, 월출/월몰 시간 및 천체 데이터

widget-system-name = 시스템
widget-system-desc = CPU, 메모리, 디스크, 네트워크 및 배터리 표시
# ---- Shared size / duration formatting ----
byte-size-b = { $value } B
byte-size-kb = { $value } KB
byte-size-mb = { $value } MB
byte-size-gb = { $value } GB
byte-size-tb = { $value } TB
duration-days-hours = { $days }일 { $hours }시간
duration-hours-minutes = { $hours }시간 { $minutes }분
duration-minutes = { $minutes }분

widget-rss-name = 뉴스
widget-rss-desc = RSS 및 Atom 뉴스 피드

widget-recent-files-name = 최근 파일
widget-recent-files-desc = Orchid에서 최근에 연 파일

widget-search-name = 통합 검색
widget-search-desc = 파일 검색, 명령 실행, 설정 열기

widget-media-name = 미디어 플레이어
widget-media-desc = 재생 중인 미디어 및 전송 제어

widget-password-name = 비밀번호
widget-password-desc = 비밀번호 데이터베이스에 액세스

widget-viewer-name = 뷰어
widget-viewer-desc = 이미지, 문서, 소스 파일 및 아카이브 보기

# ---- Weather ----
weather-condition-clear = 맑음
weather-condition-partly-cloudy = 구름 조금
weather-condition-cloudy = 흐림
weather-condition-overcast = 잔뜩 흐림
weather-condition-fog = 안개
weather-condition-drizzle = 이슬비
weather-condition-rain = 비
weather-condition-snow = 눈
weather-condition-sleet = 진눈깨비
weather-condition-thunderstorm = 뇌우
weather-condition-hail = 우박
weather-condition-windy = 바람
weather-condition-unknown = 알 수 없음
weather-day-today = 오늘
weather-day-tomorrow = 내일
weather-status-fresh = 최신
weather-status-stale = 데이터가 오래되었을 수 있음
weather-status-offline = 오프라인
weather-status-error = 날씨 로드 오류
weather-updated-just-now = 방금 업데이트됨
weather-updated-minutes = { $m }분 전 업데이트
weather-updated-hours = { $h }시간 전 업데이트
weather-updated-days = { $d }일 전 업데이트

# ---- Relative time (shared) ----
relative-just-now = 방금
relative-minutes = { $m }분 전
relative-hours = { $h }시간 전
relative-days = { $d }일 전

weather-loading = 날씨 로드 중…
weather-feels-like = 체감 { $temp }
weather-humidity-label = 습도
weather-wind-label = 바람
weather-humidity-line = { $label } { $h }%
weather-wind-line = { $label } { $speed } km/h { $dir }
weather-wind-line-no-dir = { $label } { $speed } km/h

# ---- Wind directions ----
weather-wind-n = 북
weather-wind-nne = 북북동
weather-wind-ne = 북동
weather-wind-ene = 동북동
weather-wind-e = 동
weather-wind-ese = 동남동
weather-wind-se = 남동
weather-wind-sse = 남남동
weather-wind-s = 남
weather-wind-ssw = 남남서
weather-wind-sw = 남서
weather-wind-wsw = 서남서
weather-wind-w = 서
weather-wind-wnw = 서북서
weather-wind-nw = 북서
weather-wind-nnw = 북북서

# ---- Moon ----
moon-phase-new = 신월
moon-phase-waxing-crescent = 초승달
moon-phase-first-quarter = 상현달
moon-phase-waxing-gibbous = 상현망간달
moon-phase-full = 보름달
moon-phase-waning-gibbous = 하현망간달
moon-phase-last-quarter = 하현달
moon-phase-waning-crescent = 그믐달
moon-illumination = { $pct }% 조명
moon-age = 월령: { $days }일
moon-distance = 거리: { $km } km
moon-next-full = 다음 보름달: { $date }
moon-next-new = 다음 신월: { $date }
moon-moonrise = 월출: { $time }
moon-moonset = 월몰: { $time }
moon-sunrise = 일출: { $time }
moon-sunset = 일몰: { $time }
moon-libration = Libration: { $lat }°, { $lon }°
moon-loading = 달 데이터 계산 중…

# ---- System ----
system-cpu-label = CPU
system-memory-label = 메모리
system-disk-label = 디스크 { $mount }
system-network-label = 네트워크 { $name }
system-battery-label = 배터리
system-uptime-label = 가동 시간
system-battery-charging = 충전 중
system-battery-time-remaining = { $time } 남음
system-network-rate = ↑ { $up }/s  ↓ { $down }/s
system-loading = 시스템 메트릭 로드 중…

# ---- RSS ----
rss-no-feeds = 구성된 피드 없음
rss-loading = 뉴스 로드 중…
rss-fetch-failed = 피드를 로드할 수 없습니다. 연결을 확인하고 다시 시도하세요.
rss-empty = 구성된 피드에 아직 항목이 없습니다.
recent-files-empty = 최근 파일이 없습니다. 뷰어나 파일 관리자에서 파일을 열면 여기에 표시됩니다.
rss-error-summary = { $total }개 피드 중 { $n }개 업데이트 실패
rss-item-published-minutes = { $m }분 전
rss-item-published-hours = { $h }시간 전
rss-item-published-days = { $d }일 전

# ---- Universal Search ----
search-placeholder = 파일, 명령, 설정 검색…
search-empty-state = 검색하려면 입력하세요
search-no-results = "{ $query }"에 대한 결과 없음
search-no-results-short = 결과 없음
search-sources-unconfigured = 검색 소스가 아직 구성되지 않았습니다
search-searching = 검색 중…
search-source-files = 파일
search-source-commands = 명령
search-source-settings = 설정

# ---- Command palette ----
command-palette-placeholder = 명령 실행…
command-palette-empty = 모든 명령

# ---- Registered commands ----
command.widget.create.name = 위젯 만들기
command.widget.create.desc = 작업 공간에 새 위젯 추가
command.widget.create.arg.type = 위젯 유형 ID(예: terminal, weather)

command.widget.close.name = 위젯 닫기
command.widget.close.desc = 위젯 인스턴스 닫기

command.widget.move.name = 위젯 이동
command.widget.resize.name = 위젯 크기 조정
command.widget.focus_next.name = 다음 위젯에 포커스
command.widget.show_all.name = 모든 위젯 표시
command.widget.group.dissolve.name = 위젯 그룹 해제

command.workspace.create.name = 작업 공간 만들기
command.workspace.delete.name = 작업 공간 삭제
command.workspace.switch_to.name = 작업 공간으로 전환
command.workspace.switch_next.name = 다음 작업 공간
command.workspace.switch_previous.name = 이전 작업 공간

command.terminal.split_horizontal.name = 터미널 가로 분할
command.terminal.split_vertical.name = 터미널 세로 분할
command.terminal.tab_new.name = 새 터미널 탭
command.terminal.close.name = 터미널 창 또는 탭 닫기
command.terminal.focus_next_pane.name = 다음 터미널 창에 포커스
command.terminal.focus_previous_pane.name = 이전 터미널 창에 포커스
command.terminal.tab_next.name = 다음 터미널 탭
command.terminal.tab_previous.name = 이전 터미널 탭

# ---- Settings (universal search) ----
settings.section.general = 일반
settings.section.appearance = 모양
settings.section.input = 입력
settings.section.shortcuts = 단축키
settings.section.locale = 언어
settings.section.privacy = 개인정보

# ---- Settings panel ----
settings-panel-title = 설정
settings-panel-hint = 값은 현재 읽기 전용입니다. config.toml을 직접 편집하세요. 변경 사항은 자동으로 다시 로드됩니다.
settings-panel-coming-soon = 이 섹션의 전체 설정 편집기는 아직 사용할 수 없습니다. 지금은 config.toml을 직접 편집하세요.
settings-panel-ok = 닫기

settings-open-in-editor = 편집기에서 열기
settings-open-config-file = config.toml 열기
settings-value-yes = 예
settings-value-no = 아니오
settings-value-none = 없음
settings-value-default = 기본값
settings-value-disabled = 사용 안 함
settings-value-system-default = 시스템 기본값
settings-value-hand-left = 왼손
settings-value-hand-right = 오른손
settings-value-pen-double-tap-none = 없음
settings-value-pen-double-tap-switch-tool = 도구 전환
settings-value-pen-double-tap-erase = 지우기
settings-value-sunday = 일요일
settings-value-monday = 월요일

settings-field-auto-update = 자동 업데이트
settings-field-telemetry = 원격 분석
settings-field-open-on-startup = 시작 시 열기
settings-field-theme = 테마
settings-field-density = 밀도
settings-field-font-family = 글꼴
settings-field-font-scale = 글꼴 크기
settings-field-reduce-motion = 모션 줄이기
settings-field-follow-system-theme = 시스템 테마 따르기
settings-field-dark-theme = 어두운 테마
settings-field-light-theme = 밝은 테마
settings-field-primary-hand = 주 사용 손
settings-field-mirror-edge-swipes = 가장자리 스와이프 미러링
settings-field-haptic-feedback = 햅틱 피드백
settings-field-palm-rejection = 손바닥 거부
settings-field-pen-double-tap = 펜 더블 탭
settings-field-shortcut-overrides = 단축키 재정의
settings-field-leader-key = 리더 키
settings-field-leader-timeout = 리더 시간 제한
settings-field-leader-bindings = 리더 바인딩
settings-field-language = 언어
settings-field-date-format = 날짜 형식
settings-field-time-format = 시간 형식
settings-field-first-day-of-week = 한 주의 첫날
settings-field-record-action-history = 작업 기록 저장
settings-field-history-retention-days = 기록 보존(일)
settings-field-clear-clipboard-seconds = 복사 후 클립보드 지우기

settings-field-vault-auto-lock = 금고 자동 잠금(초)
command.settings.open.name = 설정 열기
command.settings.open.desc = 설정 패널 표시
command.settings.open_config_file.name = 설정 열기
command.settings.open_config_file.desc = 기본 편집기에서 config.toml 열기
command.password.lock.name = 비밀번호 금고 잠금
command.password.lock.desc = 잠금 해제된 비밀번호 데이터베이스를 메모리에서 지우기

command.navigation.show_workspace_panel.name = 작업 공간 패널 표시
command.navigation.show_workspace_panel.desc = 작업 공간 사이드바 표시/숨기기
command.notification.show_center.name = 알림 센터 표시
command.notification.show_center.desc = 알림 센터 표시/숨기기
command.dock.show.name = 독 표시
command.dock.show.desc = 위젯 독 표시/숨기기
command.search.show_universal.name = 통합 검색
command.search.show_universal.desc = 통합 검색 열기 또는 포커스
command.onboarding.toggle_hint_mode.name = 힌트 모드 전환
command.onboarding.toggle_hint_mode.desc = 작업 공간의 제스처 힌트 표시/숨기기
navigation-workspace-panel-title = 작업 공간
notification-center-title = 알림
notification-center-placeholder = 아직 알림이 없습니다.
notification-center-clear = 모두 지우기
notification-center-tip-title = 팁
notification-center-tip-body = 오른쪽 가장자리에서 스와이프하거나 «알림 센터 표시»를 실행하여 이 패널을 엽니다.
# ---- Terminal tab bar ----
terminal-tooltip-split-h = 가로 분할 (Ctrl+Shift+H)
terminal-tooltip-split-v = 세로 분할 (Ctrl+Shift+J)
terminal-tooltip-tab-new = 새 탭 (Ctrl+Shift+T)

# ---- Media player ----
media-no-session = 재생 중인 미디어 없음
media-loading = 미디어 로드 중…
media-unsupported = 이 플랫폼에서는 미디어 제어를 사용할 수 없습니다
media-play = 재생
media-pause = 일시 정지
media-next = 다음
media-previous = 이전

# ---- Password manager ----
password-locked = 데이터베이스가 잠겨 있습니다
password-unlock-label = 마스터 비밀번호
password-unlock-placeholder = 마스터 비밀번호 입력
password-unlock-submit = 잠금 해제
password-unlock-biometric = Windows Hello
password-unlock-biometric-prompt = 비밀번호 금고 잠금 해제
password-search-placeholder = 항목 검색…
password-no-entries = 아직 항목 없음
password-copy-password = 비밀번호 복사
password-copy-username = 사용자 이름 복사
password-copy-totp = TOTP 복사
password-open-url = URL 열기
password-password-copied = 비밀번호 복사됨(30초 후 지워짐)
password-totp-copied = TOTP 복사됨(30초 후 지워짐)
password-totp-remaining = { $s }초


# ==== Viewer widget ====
widget-viewer-name = 뷰어
widget-viewer-desc = 파일 열기: 이미지, PDF, 텍스트, 아카이브
viewer-loading = 로드 중…
viewer-error = 이 파일을 표시할 수 없습니다
viewer-unsupported = 지원되지 않는 파일 형식
viewer-image-fit-screen = 화면에 맞추기
viewer-image-actual-size = 실제 크기
viewer-image-rotate = 회전
viewer-image-flip-h = 가로 뒤집기
viewer-image-flip-v = 세로 뒤집기
viewer-image-zoom-in = 확대
viewer-image-zoom-out = 축소
viewer-image-rotate-cw = 시계 방향 회전
viewer-image-rotate-ccw = 반시계 방향 회전
viewer-archive-root = (루트)
viewer-pdf-page-of = { $total }페이지 중 { $current }페이지
viewer-pdf-fit-width = 너비에 맞추기
viewer-pdf-fit-page = 페이지에 맞추기
viewer-pdf-go = 이동
viewer-pdf-info = PDF · { $current } / { $total }쪽 · { $width } × { $height } px · { $zoom }%
viewer-image-info = { $width } × { $height } · { $size } · { $format }
viewer-archive-info = { $format }, { $count }개 항목
viewer-archive-extracted-selected = { $path }(으)로 추출됨
viewer-archive-extracted-all = { $count }개 항목을 { $path }(으)로 추출함
viewer-archive-nothing-selected = 추출할 항목이 선택되지 않았습니다
viewer-archive-cannot-extract-folder = 폴더는 추출할 수 없습니다
viewer-action-failed = 뷰어 작업 실패: { $reason }
viewer-text-save-failed = 파일을 저장할 수 없습니다: { $reason }
viewer-text-read-only = 읽기 전용
viewer-text-editing = 편집 중
viewer-text-save = 저장 (Ctrl+S)
viewer-text-lines = { $count }줄
viewer-text-unsaved-title = 저장되지 않은 변경 사항
viewer-text-unsaved-body = 닫기 전에 변경 사항을 저장할까요?
viewer-text-discard = 버리기

viewer-text-dirty-indicator = 저장되지 않은 변경 사항
viewer-archive-extract-all = 모두 추출
viewer-archive-extract-selected = 선택 항목 추출
viewer-archive-preview-binary = 바이너리 파일, { $size }

# ==== File manager widget ====
widget-fm-name = 파일
widget-fm-desc = 파일 탐색, 정리 및 관리
fm-nav-back = 뒤로
fm-nav-forward = 앞으로
fm-nav-up = 위로
fm-nav-home = 홈
fm-view-icons = 아이콘
fm-view-list = 목록
fm-view-details = 세부 정보
fm-view-gallery = 갤러리
fm-sort-name = 이름
fm-sort-size = 크기
fm-sort-modified = 수정됨
fm-sort-type = 유형
fm-action-open = 열기
fm-action-open-all = 모두 열기
fm-action-open-with = 연결 프로그램…
fm-action-open-default = 기본 앱으로 열기
fm-action-open-in-viewer = Orchid Viewer에서 열기
fm-action-copy = 복사
fm-action-cut = 잘라내기
fm-action-paste = 붙여넣기
fm-action-rename = 이름 바꾸기
fm-action-delete = 삭제
fm-action-new-folder = 새 폴더
fm-action-new-tab = 새 탭
fm-action-close-tab = 탭 닫기
fm-action-select-all = 모두 선택
fm-action-deselect-all = 선택 해제
fm-action-star = 즐겨찾기
fm-action-unstar = 즐겨찾기 해제
fm-action-encrypt = 암호화
fm-action-reveal = 임시로 표시
fm-action-decrypt = 복호화
fm-action-add-tag = 태그 추가…
fm-action-remove-tag = 태그 제거
fm-action-color-label = 색상 레이블
fm-color-red = 빨강
fm-color-orange = 주황
fm-color-yellow = 노랑
fm-color-green = 초록
fm-color-blue = 파랑
fm-color-purple = 보라
fm-color-gray = 회색
fm-color-none = 색상 없음
fm-action-properties = 속성
fm-action-add-to-managed = 관리 폴더에 추가
fm-action-remove-from-managed = 관리 폴더에서 제거
fm-action-managed-policy = 관리 폴더 정책
fm-managed-policy-title = 관리 폴더 정책
fm-policy-max-size = 최대 크기
fm-policy-retention = 보존 기간
fm-policy-excludes = 제외 패턴
fm-policy-unlimited = 무제한
fm-policy-forever = 영구 보존
fm-policy-retention-days = { $days }일
fm-policy-none = 없음
fm-sidebar-managed-folder-policy = { $name } ({ $count }개 파일, { $dedup } 절약, 정책)
fm-sidebar-managed-policy-only = { $name } (정책)
fm-rename-title = 이름 바꾸기
fm-rename-ok = 확인
fm-rename-cancel = 취소
fm-dual-pane-on = 이중 창
fm-dual-pane-off = 단일 창
fm-show-hidden-on = 숨김 파일 표시
fm-show-hidden-off = 숨김 파일 숨기기
fm-click-single-on = 한 번 클릭하여 열기
fm-click-single-off = 두 번 클릭하여 열기
fm-encrypt-title = 암호 구문으로 암호화
fm-reveal-title = 표시할 암호 구문 입력
fm-decrypt-title = 복호화할 암호 구문 입력
fm-info-close = 닫기
fm-properties-title = 속성
fm-properties-kind-folder = 폴더
fm-properties-kind-file = 파일
fm-properties-type = 유형: { $kind }
fm-properties-size = 크기: { $size }
fm-properties-modified = 수정됨: { $modified }
fm-properties-mime = MIME: { $mime }
fm-tag-add-title = 태그 추가
fm-confirm-delete = { $n }개 항목을 삭제하시겠습니까?
fm-confirm-delete-permanent = { $n }개 항목을 영구 삭제하시겠습니까?
fm-status-items = { $n }개 항목
fm-status-selected = { $n }개 선택됨
fm-status-total-size = { $size }
fm-status-bar = { $items }개 항목, { $selected }개 선택됨
fm-status-managed = { $items }개 항목, { $selected }개 선택됨 · { $tracked }개 수집, { $dedup }개 중복 제거
fm-encrypted = 암호화됨: { $name }
fm-decrypted = 복호화됨: { $name }
fm-managed-added = 관리 폴더에 추가됨
fm-managed-removed = 관리 폴더에서 제거됨
fm-encryption-unavailable = 암호화를 사용할 수 없습니다
fm-passphrase-failed = 암호 구문 실패: { $reason }
fm-passphrase-invalid = 잘못된 암호 구문
fm-passphrase-required = 암호 구문이 필요합니다
fm-decryption-failed = 복호화 실패
fm-passphrase-encrypt-hint = 강력한 암호 구문을 선택하세요. 분실 시 복구할 수 없습니다.
fm-passphrase-decrypt-hint = 이 파일을 암호화할 때 사용한 암호 구문을 입력하세요.
fm-passphrase-reveal-hint = 파일은 보기 위해 임시 위치로 복호화됩니다.
fm-passphrase-biometric = Windows Hello
fm-passphrase-biometric-prompt = 암호화된 파일 잠금 해제
fm-revealed = 표시됨: { $name }
fm-managed-unavailable = 관리 폴더를 사용할 수 없습니다
fm-managed-no-selection = 관리 폴더에 추가할 폴더를 선택하세요
fm-not-managed-folder = 관리 폴더가 아닙니다
fm-managed-conflict = 관리 폴더 충돌
fm-sidebar-managed-folder = { $name }({ $count }개 파일, { $dedup }개 절약)
fm-ingest-failed = 수집 실패: { $name }
fm-quick-filter-placeholder = 필터…
fm-sidebar-favorites = 즐겨찾기
fm-sidebar-categories = 범주
fm-sidebar-managed = 관리 폴더
fm-network-placeholder = 네트워크 마운트가 구성되지 않았습니다. config.toml에 [[file-manager.network-mounts]] 항목을 추가하세요(SFTP, SMB, WebDAV, rclone을 통한 FTP).
fm-network-no-provider = 이 네트워크 위치에 등록된 파일 시스템 공급자가 없습니다.
fm-network-rclone-missing = rclone이 설치되지 않았거나 PATH에 없습니다. 필요하면 RCLONE_BIN을 설정하세요.
fm-network-invalid-mount = 이 네트워크 마운트가 잘못 구성되었습니다. config.toml의 이름과 URI를 확인하세요.
fm-network-auth-failed = 인증에 실패했습니다. config.toml의 사용자 이름과 비밀번호를 확인하세요.
fm-network-permission-denied = 이 네트워크 위치에 대한 권한이 거부되었습니다.
fm-network-connection-failed = 네트워크 호스트에 연결할 수 없습니다. URI와 네트워크를 확인하세요.
fm-ingested = 수집됨: { $name }
fm-ingesting = 수집 중: { $name }({ $count }개 활성)
fm-ingesting-count = { $count }개 파일 수집 중…
fm-copying = 복사 중: { $name }({ $percent }%)
fm-moving = 이동 중: { $name }({ $percent }%)
fm-transfer-failed = 전송 실패: { $reason }
fm-action-failed = 파일 작업 실패: { $reason }
fm-invalid-folder-name = 잘못된 폴더 이름
fm-no-provider-parent = 상위 폴더에 액세스할 수 없습니다
fm-no-parent-folder = 상위 폴더 없음
fm-selection-multiple-folders = 선택이 여러 폴더에 걸쳐 있습니다
fm-invalid-rename-target = 잘못된 이름 바꾸기 대상
fm-cannot-rename-root = 루트 이름을 바꿀 수 없습니다
fm-no-provider-path = 이 경로에 액세스할 수 없습니다
fm-empty-tag = 태그 이름은 비워 둘 수 없습니다
fm-drop-not-directory = 드롭 대상이 폴더가 아닙니다
fm-drop-unavailable = 드롭 대상을 사용할 수 없습니다
fm-type-ext-file = { $ext } 파일
fm-transfer-already-exists = 해당 이름의 파일이 이미 있습니다
fm-transfer-virtual-dest = 가상 폴더로 복사하거나 이동할 수 없습니다
fm-clipboard-copy = { $count }개 항목 붙여넣기 준비됨
fm-clipboard-cut = { $count }개 항목(잘라내기) 붙여넣기 준비됨
fm-sidebar-tags = 태그
fm-sidebar-recent = 최근
fm-sidebar-network = 네트워크
fm-sidebar-network-all = 모든 위치
fm-category-images = 이미지
fm-category-documents = 문서
fm-category-video = 동영상
fm-category-audio = 오디오
fm-category-archives = 아카이브
fm-virtual-recent = 최근
fm-virtual-starred = 즐겨찾기
fm-virtual-tags = 태그
fm-virtual-recent-empty = 최근 파일이 없습니다. 파일을 열면 여기에 표시됩니다.
fm-virtual-starred-empty = 즐겨찾기 파일이 없습니다. 컨텍스트 메뉴에서 즐겨찾기를 추가하세요.
fm-virtual-tags-empty = 태그가 있는 파일이 없습니다. 컨텍스트 메뉴에서 태그를 추가하세요.
fm-virtual-category-empty = 이 범주에서 일치하는 파일을 찾을 수 없습니다.
fm-virtual-create-denied = 가상 위치에 폴더를 만들 수 없습니다
fm-empty-folder = 이 폴더가 비어 있습니다
fm-error-access = 이 위치에 액세스할 수 없습니다


# ==== Startup shell (task 11A) ====
window-title = Orchid
startup-welcome = Orchid에 오신 것을 환영합니다
startup-subtitle = 터치 우선 컴퓨팅 환경
startup-version-label = 버전 { $version }
status-theme = 테마:
status-language = 언어:
status-density = 밀도:
density-touch = 터치
density-mouse = 마우스
density-hybrid = 하이브리드

# ---- Workspace shell (task 11B) ----
startup-get-started = 시작하기
onboarding-back = 뒤로
onboarding-next = 다음
onboarding-skip = 투어 건너뛰기
onboarding-finish = 시작하기
onboarding-step-welcome-title = Orchid에 오신 것을 환영합니다
onboarding-step-welcome-body = Orchid는 제스처, 명령, 위젯이 같은 동작의 세 가지 형태인 터치 우선 작업 공간입니다. 이 짧은 투어에서 기본을 안내합니다.
onboarding-step-workspace-title = 작업 공간
onboarding-step-workspace-body = 상단에서 작업 공간을 전환하고, 캔버스에 위젯을 배치하며, 하단 독에서 새 위젯을 추가하세요.
onboarding-step-palette-title = 명령 팔레트
onboarding-step-palette-body = Ctrl+Shift+P를 눌러 명령을 실행하세요. 각 항목에 단축키가 표시되어 사용하면서 배울 수 있습니다.
onboarding-step-gestures-title = 제스처와 힌트
onboarding-step-gestures-body = 화면 가장자리에서 스와이프하여 패널과 독을 엽니다. Win+?를 눌러 힌트 모드를 전환하고 현재 사용 가능한 기능을 확인하세요.
onboarding-hint-workspace = 왼쪽 가장자리에서 스와이프 — 작업 공간
onboarding-hint-dock = 아래 가장자리에서 위로 스와이프 — 독
onboarding-hint-gestures = Win+? — 이 힌트 전환
workspace-default-name = 메인
workspace-new = 새 작업 공간
workspace-placement-blocked-title = 여기에 위젯을 배치할 수 없습니다
workspace-placement-blocked-body = 다른 위젯과 겹치거나 그리드를 벗어납니다. 빈 칸을 선택하세요.
group-tooltip-dissolve = 위젯 그룹 해제
group-tooltip-move-left = 탭을 왼쪽으로
group-tooltip-move-right = 탭을 오른쪽으로
group-tooltip-close-tab = 그룹에서 제거
group-hint-alt-detach = Alt+드래그로 그룹에서 분리
workspace-unnamed = 작업 공간 { $n }
dock-add-label = 위젯 추가
catalog-title = 위젯 카탈로그
catalog-search-placeholder = 위젯 검색…
dock-widget-terminal = 터미널
dock-widget-weather = 날씨
dock-widget-moon = 달
dock-widget-system = 시스템
dock-widget-rss = 뉴스
dock-widget-recent-files = 최근
dock-widget-search = 검색
dock-widget-media = 미디어
dock-widget-password = 비밀번호
dock-widget-viewer = 뷰어
dock-widget-fm = 파일

viewer-no-file = 열린 파일 없음
viewer-loading-path = 로드 중: { $path }
viewer-error-with-reason = 이 파일을 표시할 수 없습니다: { $reason }
viewer-pdf-unavailable = 이 빌드에서는 PDF 지원을 사용할 수 없습니다.
viewer-image-heic-unsupported = HEIC 이미지는 아직 지원되지 않습니다
viewer-image-raw-unsupported = RAW 이미지는 아직 지원되지 않습니다
viewer-archive-select-preview = 미리 볼 파일 선택
viewer-archive-binary-preview = 바이너리 파일, { $size }

password-select-entry = 항목 선택
password-label-title = 제목
password-label-username = 사용자 이름
password-label-password = 비밀번호
password-label-url = URL
password-label-notes = 메모
password-label-totp = TOTP
password-action-copy = 복사
password-action-open = 열기
password-action-lock = 잠금
password-action-add = 추가
password-add-title = 새 항목
password-add-submit = 저장
password-add-cancel = 취소
password-generate = 생성
password-add-error-title = 제목은 필수입니다
password-entry-added = 항목 저장됨

password-username-copied = 사용자 이름 복사됨

moon-age-label = 월령
moon-distance-label = 거리
moon-next-full-label = 다음 보름달
moon-next-new-label = 다음 신월
moon-moonrise-label = 월출
moon-moonset-label = 월몰
moon-sunrise-label = 일출
moon-sunset-label = 일몰
moon-libration-label = Libration

widget-title-terminal = 터미널
widget-close-tooltip = 위젯 닫기
widget-close-confirm = { $name }을(를) 닫으시겠습니까?
action-confirm-yes = 예
action-confirm-no = 아니오

fm-confirm-title = 확인
