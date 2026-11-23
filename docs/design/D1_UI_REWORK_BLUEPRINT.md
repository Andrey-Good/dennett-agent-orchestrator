# D1 Desktop UI/UX & Dynamic Runtime Rework Blueprint

Статус: Утверждённое задание на реализацию для `@ui-author` и приёмку для `@ui-reviewer` / `@code-verifier` · 2026-11-23 · Ветка `d1/first-vertical-slice`.

Настоящий документ переводит продуктовые требования владельца ([UX_AND_PRODUCT_BACKLOG.md](UX_AND_PRODUCT_BACKLOG.md), [AGENTS.md](../../AGENTS.md)) и визуальный эталон [owner-direction-v5.md](WP-M01-003/owner-direction-v5.md) ([project-chat-native-v11-mica-1280x800.jpg](WP-M01-003/project-chat-native-v11-mica-1280x800.jpg), [project-chat-owner-v3-2048x1280.png](WP-M01-003/project-chat-owner-v3-2048x1280.png)) в точные контракты API, схемы данных, правила безопасности и покомпонентную спецификацию интерфейса поверх существующей архитектуры [80](../architecture/80_Dennett_System_Architecture_and_Runtime_Topology.md), [82](../architecture/82_Dennett_Agent_Voice_Capability_and_Integration_Architecture.md), [83](../architecture/83_Dennett_Client_Operations_Testing_and_Implementation_Blueprint.md) и спецификации [60](../specifications/60_Dennett_Desktop_Application_Business_Logic.md).

---

## 1. Факты живой разведки `agy acp 1.5.15` (основа динамического каталога)

Изолированная read-only проверка `agy acp` (`1.5.15`, `protocolVersion: 1`, `oauth-personal`) показала фактическую структуру ответов `initialize`, `session/new` и `session/set_config_option`:

1. **Подписка и профиль (`_meta.antigravity.tier`)**:
   - В `session/new` приходит `_meta.antigravity.tier = { "name": "Google AI Ultra", "id": "g1-ultra-tier", "upgradeUri": "" }`.
2. **Семейства моделей (`configOptions[id == "model"]`)**:
   - `currentValue`: `"gemini-4-neon"`.
   - `options`: список семейств с человекочитаемыми именами (`{ "value": "gemini-4-neon", "name": "Gemini 4 Neon" }`, `{ "value": "gemini-4-carbon", "name": "Gemini 4 Carbon" }`, `{ "value": "gemini-4-argon", "name": "Gemini 4 Argon" }`, `{ "value": "gemini-4-hydrogen", "name": "Gemini 4 Hydrogen" }`, `{ "value": "gemini-pro-orchestrator-neon", "name": "Orchestrator (Neon)" }`, `{ "value": "gemini-pro-orchestrator", "name": "Orchestrator (Carbon)" }`, `{ "value": "gemini-pro-orchestrator-argon", "name": "Orchestrator (Argon)" }`, `{ "value": "gemini-3.8-flash", "name": "Gemini 3.8 Flash" }`, `{ "value": "claude-sonnet-4-6", "name": "Claude Sonnet 4.6 (Thinking)" }`, `{ "value": "claude-opus-4-6-thinking", "name": "Claude Opus 4.6 (Thinking)" }`, `{ "value": "gpt-oss-120b-medium", "name": "GPT-OSS 120B (Medium)" }`).
3. **Уровни размышления (`configOptions[id == "thought_level"]` и `models.availableModels`)**:
   - Глобальные опции `thought_level`: `low` (`"Low"`), `medium` (`"Medium"`), `high` (`"High"`), `xhigh` (`"Extra"`).
   - **Критическая особенность протокола**: набор доступных `thought_level` зависит от выбранной модели:
     - `gemini-4-neon` и `gemini-4-hydrogen` поддерживают `[low, medium, high, xhigh]`;
     - `gemini-4-carbon`, `gemini-4-argon`, `gemini-3.8-flash` поддерживают только `[low, medium, high]` (вызов `set_config_option("model", "gemini-4-carbon")` возвращает обновлённый `configOptions` уже без `xhigh`);
     - `claude-opus-4-6-thinking`, `claude-sonnet-4-6`, `Orchestrator (*)`, `gpt-oss-120b-medium` **вообще не имеют `thought_level` в `configOptions`** после выбора модели (уровень размышления встроен в саму модель).
   - Вывод для `adapters/antigravity-acp`:
     - При разборе `session/new` вычислять `supported_thinking_levels` для каждого семейства `family` из `configOptions[model].options` по наличию суффиксов `{family}-{low|medium|high|xhigh}` в `models.availableModels`.
     - В `AntigravityAcpRuntime::apply_model_and_thinking` анализировать `configOptions`, возвращённые из `session/new` / `session/load` / `session/set_config_option("model", ...)`: если `thought_level` отсутствует в `configOptions` — **не вызывать** `set_config_option("thought_level", ...)`; если запрошенный уровень (`xhigh`) отсутствует среди допустимых опций текущей модели (`[low, medium, high]`) — мягко понижать до максимального поддерживаемого (`high`), не роняя ход ошибкой JSON-RPC `-32602`. Также поддерживать передачу полного суффиксного идентификатора (например `"gemini-4-carbon-high"` -> `model = "gemini-4-carbon"`, `thought_level = "high"` при отсутствии явного `thinking_level`).
4. **Квоты (`quotas`)**:
   - В `agy 1.5.15` машинно-читаемые окна остатка квот в `initialize` / `session/new` не публикуются (`quotas: []`). Парсер должен проверять опциональный `_meta.antigravity.quotas` (на случай появления в следующих версиях `agy`), но при отсутствии — честно возвращать пустой массив `quotas: []`, чтобы `<QuotaRings />` чисто скрывался без фейковых чисел `100`.

---

## 2. Визуальный порт 1-в-1 с эталоном `WP-M01-003` и материальная иерархия

### 2.1. Пакеты и иконки
- В `apps/desktop/package.json` добавить проверенные библиотеки из архивного эталона (`origin/archive/before-code-reset-2026-09-27:apps/desktop/package.json`):
  - `@phosphor-icons/react` (`^2.1.10`)
  - `react-markdown` (`^10.1.0`)
  - `remark-gfm` (`^4.0.1`)
- Обязательно обновить `apps/desktop/pnpm-lock.yaml` через `pnpm --dir apps/desktop install`, чтобы CI-проверка `pnpm --dir apps/desktop install --frozen-lockfile` проходила на Linux и Windows.
- Перенести монохромные иконки приложения из `origin/archive/before-code-reset-2026-09-27:apps/desktop/src-tauri/icons/` при необходимости синхронизации ассетов.
- Для рендера ответов ассистента использовать `<ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>` — **строго 0 `dangerouslySetInnerHTML`** и полное экранирование сырого HTML.

### 2.2. Поверхности, цвета, радиусы и запрет `<select>`
- **Шрифт и шкала ([owner-direction-v5.md](WP-M01-003/owner-direction-v5.md))**:
  - `Inter, "Segoe UI Variable Text", "Segoe UI", ui-sans-serif, system-ui, sans-serif`; моноширинный `"Cascadia Code", Consolas, ui-monospace, monospace`.
  - Текст диалога и поля ввода композера: `14px–15px` (`line-height: 1.66`); навигация проектов/чатов: `13.5px–14px`; заголовки разделов и вторичные метаданные: `11px–12px`.
  - Первичные названия проектов, чатов, аккаунта, активных кнопок и файлов — полупрозрачный белый `rgba(255, 255, 255, 0.92)` (`#f1f1f1`), вторичные подписи и время — спокойный серый `#a4a4a4` / `rgba(255, 255, 255, 0.52)`. Акцентная палитра — строго монохромная графитовая (цветные точки и плашки допускаются **только** для функциональных индикаторов здоровья/ошибок/SIMULATED).
- **Режим нативного окна Tauri (`html.native-shell`, когда `isTauriRuntime() === true`)**:
  - `html.native-shell, html.native-shell body, html.native-shell #root, html.native-shell .workbench` имеют `background: transparent`.
  - **Строгий запрет `backdrop-filter`** на `.workbench`, `.chat-card`, `.composer`, `.workspace-panel`, `.chat-row.is-active` внутри `html.native-shell` (Windows 11 Mica сама формирует размытие обоев; повторный `backdrop-filter` в WebView2 превращает поверхность в глухую серую плиту).
  - Нижний бесшовный слой Mica (`background: transparent`, без рамок и разделительных коробок): `.titlebar`, `.activity-rail`, `.project-sidebar`, `.user-settings-bar`.
  - Центральная карточка чата (`.chat-card`): скруглённая карточка (`border-radius: 20px`), `background: rgba(24, 24, 24, 0.36)` (`#181818` @ 36%), тонкий контур `1px solid rgba(255, 255, 255, 0.12)`.
  - Приподнятые поверхности без рамок (`.composer`, пузырь сообщения пользователя `.message--user .message-copy`, правая карточка `.workspace-panel`): `border: 0`, `background: rgba(45, 45, 45, 0.24)` (`#2d2d2d` @ 24% локальной плотности), радиусы: композер `24px`, пузырь пользователя `19px`, правая карточка `20px`.
  - Выбранный чат в сайдбаре (`.chat-row.is-active`): светлая полупрозрачная белая подсветка `rgba(255, 255, 255, 0.14)` (`border-radius: 11px`), светлее окружающего фона Mica.
- **Режим браузерного предпросмотра и ревью (`html.browser-mica-preview`, строго когда `!isTauriRuntime()`)**:
  - Чтобы открытие Vite в встроенном браузере (`@ui-reviewer`) выглядело 1-в-1 как нативный скриншот `project-chat-native-v11-mica-1280x800.jpg` (а не на белом или плоском фоне), применять для `html.browser-mica-preview` глубокий тёмно-графитовый Mica-фон оболочки (`radial-gradient(120% 90% at 12% 8%, #1d1f24 0%, #121316 52%, #0d0e10 100%)`), плотность центральной карточки `rgba(22, 22, 24, 0.72)` и приподнятых карточек `rgba(45, 45, 45, 0.56)`.
- **Растворение текста у верхнего края чата**:
  - На скролл-контейнере `.conversation` использовать компактный `mask-image: linear-gradient(to bottom, transparent 0%, rgba(0, 0, 0, 0.72) 18px, #000 36px, #000 100%)` (и `-webkit-mask-image`), чтобы прокручиваемый текст мягко растворялся до верхнего края карточки и останавливался левее физического скроллбара без `backdrop-filter`.
- **Полный запрет системных `<select>`**:
  - Во всём DOM приложения `document.querySelectorAll("select").length === 0`. Все выпадающие списки (модель, уровень размышления, режим записи в папку, создание проекта, MIME-тип артефакта) реализуются через кастомные полупрозрачные тёмные поповеры (`.floating-popover`: `background: rgba(34, 34, 34, 0.95)`, `border: 1px solid rgba(255, 255, 255, 0.18)`, `border-radius: 15px`, `box-shadow: 0 18px 48px rgba(0, 0, 0, 0.45)`, закрытие по `Escape` и клику снаружи, возврат фокуса на триггер).

---

## 3. Трёхчастная левая геометрия, полоска пользователя/настроек и `<QuotaRings />`

### 3.1. Сетка окна (`.workbench`)
```text
Строки:   48px (titlebar)  |  minmax(0, 1fr) (основная область)  |  46px (нижняя полоска слева)
Колонки:  52px (rail)      |  264px (projects/chats, 0px при Ctrl+B)  |  minmax(0, 1fr) (workspace-stage)
```
- `.titlebar`: `grid-column: 1 / -1; grid-row: 1;`
- `.activity-rail`: `grid-column: 1; grid-row: 2;` — **физически заканчивается над строкой 3** (за 46px до низа окна). Тонкая вертикальная риска справа `.activity-rail::after` (`top: 14px; bottom: 8px; right: 0; width: 1px; background: rgba(255,255,255,0.09)`) также обрывается выше нижней полоски! Вверху `.activity-rail` — монохромный знак бренда `.rail-brand` (`BracketsCurly`) и разделы (`ChatsCircle` — Чаты/Проекты, `Browsers`/`Sidebar` — переключение правой панели Workspace; будущие разделы `ListChecks`/`Plug` аккуратно приглушены с честным тултипом этапа).
- `.project-sidebar`: `grid-column: 2; grid-row: 2;` — также **физически заканчивается над строкой 3**; скролл списка проектов и чатов никогда не наезжает на нижнюю полоску пользователя.
- `.workspace-stage` (сцена карточек): `grid-column: 3; grid-row: 2 / 4;` — тянется до самого низа окна (`padding: 0 12px 12px 0`), вмещая рядом друг с другом скруглённую `.chat-card` и (когда открыта) скруглённую `.workspace-panel` с прозрачным зазором Mica `12px` между ними!
- `.user-settings-bar`: `grid-column: 1 / 3; grid-row: 3;` — в нижнем зазоре под **обеими** левыми колонками, прижатая влево к физическому краю окна (`padding: 0 10px 6px 11px`), **полностью без рамок, без фона и без верхней разделительной линии** прямо на бесшовном Mica (`background: transparent; border: 0;`).

### 3.2. Полоска пользователя и окно `Настройки и подключения`
- **Слева в `.user-settings-bar`**:
  - Монохромный бейдж-аватар `28×28px` (`border-radius: 9px`, фон `#ededed`, текст `#181818`, первая буква имени профиля) + реальная подпись пользователя/профиля: имя учётной записи ОС из `RuntimeCatalogResponse.account_label` (например `urako` или `"Владелец"` при отсутствии) и компактная вторичная метка тарифа/профиля (`Google AI Ultra` / `Gemini API key` / `Тестовый контур`).
  - **Строгий запрет**: нигде в нижней полоске, шапке или композере не выводить технические строки вида `"Агент Demo D1 · v1"`.
- **Справа в `.user-settings-bar`**:
  - Кнопка-шестерёнка `<GearSix size={17} />` (`30×30px`, `aria-label="Настройки и подключения"`, `data-testid="settings-trigger"`).
- **Поведение при свёрнутом сайдбаре (`Ctrl+B` -> `.sidebar-collapsed`, колонка 2 = `0px`)**:
  - `.user-settings-bar` плавно сжимается до `52px`: текстовая подпись скрывается, отображается компактная кнопка шестерёнки/профиля по центру рейла, открывающая то же самое окно настроек в 1 клик.
- **Полупрозрачная панель `Настройки и подключения` (`Providers & Connections`)**:
  - Открывается кликом по шестерёнке (закрывается по `Escape`, кнопке `X` или клику снаружи).
  - Показывает честную сводку:
    1. **Движок и авторизация**: активный адаптер (`Google Antigravity · agy acp <версия>` или `Simulated Runtime`), активный профиль входа (`Google AI Ultra · oauth-personal` vs `Gemini API key`), кнопка **«Обновить список моделей»** (`GET /v1/runtime/catalog?refresh=true`).
    2. **Сервер управления и память**: адрес `dennett-head` (с возможностью сменить/переподключить loopback URL), статус PostgreSQL (`dennett_control`) и независимого `dennett-memoryd`, размер очереди `outbox`.
    3. **Хранилище секретов (`Secret Broker`)**: лаконичная заметка, что интерактивный OAuth изолирован в системном keyring `agy`, а управление API-ключами сторонних провайдеров через системный Secret Broker относится к этапам D3–D5 (ключи никогда не хранятся в открытом состоянии UI).

### 3.3. Компактные кольца лимитов (`<QuotaRings />`)
- Расположение: в самом низу `.activity-rail` (`margin-top: auto; padding-bottom: 6px`), непосредственно над `.user-settings-bar`.
- Принимает `quotas: RuntimeQuotaWindow[]` из `RuntimeCatalogResponse`.
- **Анти-мусорный дизайн**:
  - Если `quotas.length === 0` (штатное поведение для `agy 1.5.15`) — компонент возвращает `null` (**полностью скрыт**, никаких фейковых `100` или пустых рамок).
  - Если `quotas.length > 0` — рисует до 2 компактных индикаторов по вертикали внутри рейла `52px`:
    - Сверху короткая метка окна (`9.5px`, цвет `#a4a4a4`: например `5ч`, `нед`).
    - Ниже тонкое SVG-кольцо прогресса (`26×26px`, `strokeWidth="2.2"`, трек `rgba(255,255,255,0.12)`, дуга `rgba(255,255,255,0.88)` или `#f3c677` при остатке `<= 15`) с **голым целым числом** внутри (`93` — **строго без знака `%`** и без слов `"процентов"`, `"дневной"`, `"недельный"`).

---

## 4. Нулевое трение для чатов и проектов + динамические области памяти

### 4.1. Мгновенный «Новый чат», авто-заголовок и inline-переименование
1. **Создание в 1 клик (`+` у проекта, кнопка в пустом состоянии, `Ctrl+K`, глобальный хоткей `Ctrl+N`)**:
   - **Никаких полей ввода названия чата при создании!**
   - Сразу вызывает `POST /v1/sessions` с `title: "Новый чат"` (привязывая к текущему активному проекту; если проектов ещё вообще нет — автоматически создаёт проект «Пока без папки» и открывает в нём пустой чат!), делает сессию активной и переводит фокус в `<textarea>` композера (`requestAnimationFrame(() => composerRef.current?.focus())`).
2. **Авто-заголовок по первому сообщению**:
   - В `HeadService::send_message`: если у сессии **ещё нет ни одного пользовательского сообщения** (`prior_history` не содержит `MessageRole::User`) **и** текущий `session.title` равен дефолтному (`"Новый чат"` / `"Новый разговор"` / `"Untitled"`), `Head` автоматически формирует чистое название из первой непустой строки `req.content` (убирая ведущие `#`, `*`, `-`, `` ` ``, схлопывая пробелы и аккуратно обрезая по границе слова до ~52 символов с `…`), сохраняет `UPDATE sessions SET title = $2, updated_at = now() WHERE session_id = $1` вместе с записью сообщения пользователя и возвращает актуальный `session_title: String` в `SendMessageResponse`.
   - Если пользователь вручную переименовал пустой чат до отправки первого сообщения — пользовательское имя сохраняется и не перезаписывается.
3. **Ручное inline-переименование чата (`PATCH /v1/sessions/{session_id}`)**:
   - Эндпоинт `PATCH /v1/sessions/{session_id}` с `UpdateSessionRequest { title: String }` (валидация `1..=256` байт без NUL, обновление `sessions.title` и `updated_at`, запись `session_renamed` в `audit_events`, возврат `SessionRecord`).
   - В списке чатов: двойной клик по названию, клавиша `F2` на сфокусированной строке **или** клик по аккуратной иконке карандаша (`PencilSimple`, `aria-label="Переименовать чат ..."`, появляется при hover/focus строки) превращает заголовок в inline `<input>`: `Enter` сохраняет, `Escape` отменяет и возвращает прежнее имя, `Blur` сохраняет при непустом изменении.

### 4.2. Нулевое трение при создании проекта («Выбрать папку…» / «Пока без папки»)
1. **Миграция БД и контракты (`ProjectRecord`)**:
   - Добавить forward-only миграцию `services/head/migrations/0002_projects_nullable_workspace.sql` (`LATEST_CONTROL_SCHEMA_VERSION = 2`; обновить проверку `schema_version` в `crates/dennett-testkit/tests/head_control_e2e_integration.rs`):
     ```sql
     ALTER TABLE projects ALTER COLUMN workspace_path DROP NOT NULL;
     ALTER TABLE projects DROP CONSTRAINT IF EXISTS projects_workspace_path_check;
     ALTER TABLE projects ADD CONSTRAINT projects_workspace_path_check
         CHECK (workspace_path IS NULL OR (octet_length(workspace_path) > 0 AND octet_length(workspace_path) <= 1024));
     ```
   - В `dennett-contracts`:
     - `ProjectRecord.workspace_path: Option<String>` (`null` в JSON, когда проект создан «Пока без папки»).
     - `CreateProjectRequest { name: String, workspace_path: Option<String>, memory_scope: Option<MemoryScope> }`: если `memory_scope == None`, `HeadService::create_project` сам генерирует валидный уникальный `MemoryScope` вида `project:<ascii-slug>-<short8>` (пользователь в UI **никогда** не видит и не вводит `memoryScope`).
     - `PATCH /v1/projects/{project_id}` с `UpdateProjectRequest { name: Option<String>, workspace_path: Option<String> }` — поддерживает inline-переименование проекта и привязку/смену рабочей папки (`Attach / Rebind Folder` по [контракту C](../specifications/contracts/C_Project_Lifecycle_Contract.md)).
2. **Поведение для проекта «Пока без папки» (`workspace_path == None`)**:
   - В `HeadService::send_message`: если `project.workspace_path` равен `None`, `Head` создаёт изолированную пустую техническую директорию `<managed_root>/detached-projects/<project_id>` для `cwd` процесса `agy acp` и принудительно передаёт `WorkspaceWritePolicy::DenyAll` в `AntigravityAcpRuntime` и `PolicyEngine` (агент свободно ведёт диалог, работает с `dennett-memory-mcp` и создаёт inline-артефакты сессии, но не пишет в файловую систему до привязки реальной папки). Когда пользователь позже привязывает папку через `PATCH /v1/projects/{id}`, `AntigravityAcpRuntime` автоматически обнаруживает смену `workspace_cwd` и выполняет честный rehydrate сессии в новую папку.
3. **Кастомное полупрозрачное меню «Новый проект» (`.project-create-menu`)**:
   - Кнопка `+` (`aria-label="Новый проект"`, видимая с лёгкой прозрачностью `0.75` в покое и `1.0` при наведении — не прятать в `opacity: 0`!) открывает полупрозрачный поповер **строго из двух пунктов без единого текстового поля**:
     - **«Выбрать папку…»** (`FolderOpen` + подпись *«Открыть существующую папку или репозиторий»*):
       - В нативном окне Tauri (`isTauriRuntime()`): вызывает Tauri-команду `pick_workspace_folder` (через крейт `rfd`, на Linux собирать с `default-features = false, features = ["gtk3"]` под уже установленный в CI `libgtk-3-dev`, на Windows — штатный `IFileOpenDialog`). Возвращает `Option<String>` без расширения прав WebView (`capabilities/default.json` остаётся неизменным!). После выбора папки проект мгновенно создаётся с именем `basename(path)`, привязанным `workspace_path` и готовым пустым чатом!
       - В dev-режиме браузера (`!isTauriRuntime()`): открывает аккуратное полупрозрачное модальное окно выбора локальной директории (показывающее список папок через dev-only Vite middleware `/__dennett_dev_dirs` с быстрым переходом по пути), чтобы `@ui-reviewer` в браузере мог выбрать любую временную папку с тестовыми файлами в 1–2 клика без системного Tauri IPC.
     - **«Пока без папки»** (`FolderPlus` + подпись *«Начать чат сразу, привязать папку позже»*):
       - **В 1 клик** создаёт проект (`name: "Новый проект"`, либо `"Новый проект N"` при совпадении, `workspace_path: null`), создаёт в нём пустой чат `"Новый чат"` и фокусирует поле ввода!
   - Заголовок проекта в сайдбаре поддерживает inline-переименование (`DoubleClick` / `F2` / иконка `PencilSimple` -> `PATCH /v1/projects/{id}`) и действие **«Привязать папку…»** (`FolderOpen`), если `workspace_path == null`.

### 4.3. Динамическая авторизация областей памяти (`project:<slug>`) без `403 ForbiddenScope`
- При создании нового проекта в живом `demo-d1` или браузерном ревью агент получает уникальную область памяти `project:<slug>-<id>`. Статический токен с фиксированным списком областей вызвал бы `403 ForbiddenScope` в `dennett-memoryd`.
- **Архитектурное решение (строгий least-privilege без ослабления `memoryd` и без зависимости `Head -> dennett-memory-core`)**:
  - `dennett-memoryd` уже поддерживает криптографически подписанные HMAC-SHA256 токены формата `dnt1.<base64url-claims>.<base64url-hmac>` (`auth_config.hmac_secret`).
  - Добавить в `dennett-head` опциональную переменную окружения `DENNETT_MEMORY_HMAC_SECRET` (и флаг `--memory-hmac-secret`, используя уже присутствующие в workspace `hmac`, `sha2`, `base64`).
  - Когда `DENNETT_MEMORY_HMAC_SECRET` задан:
    - Перед запуском/возобновлением сессии `HeadService` выпускает короткоживущий (`now + 2h`) подписанный `dnt1.`-токен с `principal_id = "agent:<agent_id>"` и `allowed_scopes = agent.current_config.allowed_memory_scopes` (ровно область текущего проекта!) и передаёт его в `DENNETT_MEMORY_TOKEN` для `dennett-memory-mcp` (одновременно добавляя токен в список секретов редактирования!).
    - Для собственных вызовов `HeadService` (`commit_memory_note`, `deliver_single_outbox_item`, `search_memory`) `HeadService` подписывает `dnt1.`-токен на целевую область `[scope]` (или список областей зарегистрированных проектов при поиске без фильтра).
  - Когда `DENNETT_MEMORY_HMAC_SECRET` не задан, `HeadService` работает со статическим `DENNETT_MEMORY_TOKEN` без изменений (100% обратная совместимость со всеми существующими интеграционными тестами).
  - В `ops/runner/src/dennett_runner/cli.py` (`demo-d1`) генерировать `hmac_secret` в `DENNETT_MEMORY_AUTH_CONFIG` для `memoryd` и передавать `DENNETT_MEMORY_HMAC_SECRET` в `dennett-head`. Также убрать из `demo-d1` технический мусор `"Demo D1"` / `"Агент Demo D1"` / `"Демо-разговор D1"`, предсоздавая чистый проект `"Рабочее пространство"` с агентом `"Проектный агент"` и пустым чатом `"Новый чат"`, а перед сборкой `dennett-desktop` проверять, не заблокирован ли `dennett-desktop.exe` запущенным процессом.

---

## 5. Динамический каталог моделей и уровней размышления (`Head` <-> `agy acp`)

1. **Контракты в `crates/dennett-contracts/src/control.rs`**:
   - `RuntimeThinkingOption { value: ThinkingLevel, label: String }`
   - `RuntimeModelOption { model_id: String, label: String, supported_thinking_levels: Vec<ThinkingLevel>, default_thinking_level: Option<ThinkingLevel> }`
   - `RuntimeQuotaWindow { label: String, remaining_percent: u8 }`
   - `RuntimeCatalogResponse { adapter_id, auth_profile, is_simulated, agy_version, tier, account_label, default_model_id, default_thinking_level, models, thinking_options, quotas, probed_at }`
2. **Расширение `RuntimeDescriptor` (`adapters/antigravity-acp`)**:
   - Расширить `AcpSessionInfo` и `RuntimeDescriptor`, сохраняя структурированные `models: Vec<RuntimeModelOption>`, `thinking_options: Vec<RuntimeThinkingOption>`, `default_model_id: Option<String>`, `default_thinking_level: Option<ThinkingLevel>` и `quotas: Vec<RuntimeQuotaWindow>` напрямую из `session/new` (`configOptions` + `models.availableModels` + `_meta.antigravity`).
   - В `SimulatedRuntime::probe()` возвращать честный `SIMULATED` каталог (`is_simulated: true`, `tier: Some("SIMULATED_CI_ONLY")`, модели `sim-model-alpha` / `sim-model-beta` с `[Low, Medium, High, XHigh]`).
   - **Полная зачистка хардкода**: удалить `MODEL_OPTIONS` и любые упоминания несуществующих `"Gemini 3.8 Pro"` / `"Gemini 2.5"` из всего репозитория (`rg -i "gemini 3\.8 pro|gemini-3\.8-pro|gemini 2\.5|gemini-2\.5"` обязан давать 0 совпадений).
3. **Эндпоинт `GET /v1/runtime/catalog` (`?refresh=true`)**:
   - Кеширует успешный результат `runtime.probe()` в памяти `HeadService` (TTL 5 минут, сериализация конкурентных запросов через `Mutex`), при `?refresh=true` принудительно перечитывает `runtime.probe()`.
   - Пробрасывается в `HeadClient::get_runtime_catalog(refresh: bool)` и Tauri-команду `get_runtime_catalog(refresh: Option<bool>)`.
4. **Два полупрозрачных селектора в композере**:
   - В нижней панели `.composer`:
     - Слева — полупрозрачный пилл политики доступа к файлам (`ShieldCheck`: *«Спрашивать запись»* / *«Запись в папку»* / *«Только чтение»*, обновляющий `workspace_write_policy` агента).
     - Справа (перед круглой кнопкой отправки `ArrowUp` / остановки `Stop`) — **два отдельных кастомных триггера с полупрозрачными поповерами**:
       1. **Селектор модели** (`data-testid="model-selector-trigger"`, `aria-label="Модель агента"`): показывает человекочитаемое имя текущей модели (или дефолтной модели движка из `RuntimeCatalogResponse.default_model_id`) и открывает полупрозрачный список `catalog.models`.
       2. **Селектор уровня размышления** (`data-testid="thinking-selector-trigger"`, `aria-label="Уровень размышления"`): показывает `Low` / `Medium` / `High` / `Extra` из `catalog.thinking_options`, отфильтрованных по `supported_thinking_levels` выбранной модели. Если у модели уровень встроен (`supported_thinking_levels` пуст, например `Claude Opus 4.6 (Thinking)`), показывать аккуратную неактивную метку `Thinking` с поясняющим тултипом.

---

## 6. Правая карточка `Workspace`, верхний поповер статистики и Dev HTTP Bridge

### 6.1. Безопасный просмотр файлов рабочей папки (`GET /v1/projects/{project_id}/files`)
- Возвращает `ListProjectFilesResponse { project_id, workspace_bound, workspace_path, entries, truncated }`.
- **Требования безопасности (`Policy` & `Sharp Edges`)**:
  - Если `project.workspace_path == None`: возвращает `200 OK` с `workspace_bound: false, entries: []`.
  - Выполняется в `tokio::task::spawn_blocking`.
  - Канонизирует корень проекта и проверяет `fs::symlink_metadata` на корне и на каждом узле обхода **до** спуска в подпапку:
    - Отклоняет пути с `..`, NUL-байтами, UNC/device-префиксами (`\\?\UNC\`, `\\.\`).
    - Отклоняет/пропускает любые симлинки (`file_type().is_symlink()`) **и Windows NTFS reparse points / junctions** (`#[cfg(windows)] MetadataExt::file_attributes(meta) & 0x0400 != 0`)! Если сам корень проекта или запрошенный `subpath` является симлинком/junction или выходит за пределы `canonical_root` — возвращать `403 AccessDenied`!
  - Жёсткие лимиты против DoS: глубина обхода `<= 3`, максимум возвращаемых записей `200`, максимум просмотренных узлов ФС `1500` (при достижении выставлять `truncated: true`), пропуск технических каталогов `.git`, `node_modules`, `target`, `.venv`.

### 6.2. Честная правая карточка `Workspace` (`.workspace-panel`)
- Открывается/скрывается кнопкой в `.titlebar__right`, кнопкой в `.activity-rail` или хоткеем `Ctrl+\`.
- Является **соседней скруглённой карточкой** в `.workspace-stage`: при открытии центральная `.chat-card` плавно сжимается влево с прозрачным зазором `12px`, никогда не перекрывая сообщения и скроллбар.
- **Полное удаление бутафории**: удалить секции `"Среда и движок"` (`"PostgreSQL: подключён"`) и сырой лог `"Диагностика"` из правой панели!
- Содержимое правой карточки:
  1. **Файлы проекта**: реальный список из `GET /v1/projects/{id}/files` (папки и файлы с относительным путём и размером; кнопка обновления; если `workspace_bound == false` — спокойное сообщение *«Папка не привязана»* и кнопка **«Привязать папку…»**).
  2. **Артефакты сессии**: список реальных `ArtifactRecord` сессии + создание нового артефакта (с кастомным поповером MIME-типа, без `<select>`) + безопасный предпросмотр выбранного артефакта.

### 6.3. Верхний компактный индикатор и поповер живой статистики
- В `.titlebar__right` размещается компактная кнопка-пилл статуса (`data-testid="system-status-pill"`, индикаторная точка + название тарифа/движка, плюс отдельный бейдж `SIMULATED`, если `runtime_is_simulated == true`).
- Клик по пиллу открывает полупрозрачный поповер **«Состояние системы и памяти»** (`.stats-popover`):
  - **Движок**: `Antigravity ACP <версия>` (или `Simulated Runtime`), тариф (`Google AI Ultra`), статус квот (`—`, если движок не публикует).
  - **Память `memoryd`**: статус (`Активна` / `Недоступна`), число актуальных заметок в области активного проекта (через `POST /v1/memory/search` по `activeProject.memory_scope`), очередь `outbox` (`pending_outbox_count`) и кнопка **«Отправить очередь»** (`flushMemoryOutbox`), если есть отложенные записи или деградация.
  - **Сессия и безопасность**: число ходов (`runs`) в текущей сессии, число проверенных вне модели решений политики (`permission_decisions.length`).
- Крупный предупреждающий баннер внутри `.chat-card` показывается **только** при реальной деградации/ошибке (падение `memoryd` по V02, ошибка запуска или явная отмена хода).

### 6.4. Dev-Only Vite Proxy + `createDevHttpBridge()` (живой браузерный стенд без ослабления CSP Tauri)
- **Tauri Production CSP и Capabilities**: `apps/desktop/src-tauri/tauri.conf.json` (`connect-src 'self' ipc: http://ipc.localhost https://ipc.localhost`) и `apps/desktop/src-tauri/capabilities/default.json` остаются **100% неизменными и запертыми на `ipc:`**.
- **Vite Dev Server (`apps/desktop/vite.config.ts`)**:
  - Настроить dev-only proxy `/__dennett_dev_head` -> `process.env.DENNETT_HEAD_URL || "http://127.0.0.1:41771"` (переписывая префикс `/__dennett_dev_head` -> `""`) и dev-only middleware `/__dennett_dev_dirs` (строго в `configureServer`, только при локальном `vite dev`, никогда не попадает в `vite build`!).
  - Не фиксировать `strictPort: true` при передаче `--port` из CLI (или оставить дефолт `1420` с поддержкой переопределения порта для параллельной проверки `@ui-reviewer`).
- **Выбор моста в `apps/desktop/src/App.tsx`**:
  1. Если передан `props.bridge` (все тесты Vitest) -> использовать `props.bridge`.
  2. Иначе если `isTauriRuntime()` -> `createTauriBridge()`.
  3. Иначе если `import.meta.env.DEV && !new URLSearchParams(window.location.search).has("mock")` -> `createDevHttpBridge("/__dennett_dev_head")` (полная реализация `DesktopBridge` поверх `fetch` к живому `dennett-head` через Vite proxy!).
  4. Иначе -> `createBrowserPreviewBridge()`.

---

## 7. Сводный чек-лист приёмки для `@ui-author`, `@ui-reviewer` и `@code-verifier`

1. **Визуал и материалы**: 1-в-1 соответствие [owner-direction-v5.md](WP-M01-003/owner-direction-v5.md) и [project-chat-native-v11-mica-1280x800.jpg](WP-M01-003/project-chat-native-v11-mica-1280x800.jpg); `0` элементов `<select>` в DOM; `0` `backdrop-filter` в `html.native-shell`; реалистичный тёмный Mica-фон в `html.browser-mica-preview`; русскоязычный продуктовый интерфейс без технического жаргона.
2. **Левая геометрия и аккаунт**: Activity Rail и список проектов заканчиваются выше нижней полоски; под обеими колонками от самого левого края без рамок и разделителей лежит полоска пользователя (аватар + реальный профиль/тариф слева, шестерёнка настроек справа); `0` строк `"Агент Demo D1 · v1"`.
3. **Квоты (`<QuotaRings />`)**: слот над полоской пользователя внизу Activity Rail; короткие метки (`5ч`, `нед`) + SVG-кольцо + голое число без `%`; при `quotas: []` чисто скрыт без фейковых `100`; покрыт unit-тестами.
4. **Чаты**: 1-клик «Новый чат» (`+` / `Ctrl+N`) без запроса имени -> мгновенный фокус в поле ввода -> авто-заголовок по первому сообщению в `Head` и UI -> inline-переименование (`DoubleClick` / `F2` / иконка, `Enter` / `Escape`) через `PATCH /v1/sessions/{id}`; сохранность черновиков между чатами.
5. **Проекты и память**: «Новый проект» открывает полупрозрачный выбор **«Выбрать папку…»** (`rfd` в Tauri / браузерный dev-пикер в Vite) и **«Пока без папки»** (1 клик, `workspace_path: null`), без текстовых полей пути `C:\...` и `memoryScope`; inline-переименование и «Привязать папку…» через `PATCH /v1/projects/{id}`; динамические HMAC-токены `dnt1.` исключают `403 ForbiddenScope` на любых новых проектах при сохранении межпроектной изоляции памяти.
6. **Модели и размышления**: `GET /v1/runtime/catalog` динамически отдаёт реальные модели и `thought_level` из `agy acp`; `0` упоминаний `"Gemini 3.8 Pro"` / `"Gemini 2.5"` в коде; переключение `Claude Opus 4.6 (Thinking)` и `Gemini 4 Carbon` работает без ошибок протокола `thought_level`.
7. **Правая панель, статы и безопасность**: правая скруглённая карточка сдвигает чат влево с зазором Mica и показывает только реальные файлы (`GET /v1/projects/{id}/files` с защитой от `..`, симлинков и NTFS reparse points/junctions + лимит глубины/узлов) и реальные артефакты; диагностика и статус `memoryd`/outbox вынесены в верхний поповер статуса; границы крейтов ([TEST_CATALOGUE_AND_QUALITY_GATES.md](../testing/TEST_CATALOGUE_AND_QUALITY_GATES.md), [ANTIGRAVITY_RUNTIME_ACCEPTANCE.md](../testing/ANTIGRAVITY_RUNTIME_ACCEPTANCE.md)) и CI полностью зелёные.
