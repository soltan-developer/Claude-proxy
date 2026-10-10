# راهنمای توسعهٔ Claude Proxy

این راهنما برای برنامه‌نویسی نوشته شده که می‌خواهد سورس Claude Proxy را تحویل بگیرد، اجرا کند، اشکال‌زدایی کند یا قابلیت تازه‌ای به آن اضافه کند. راهنمای کاربری و مراحل اتصال Claude در [`README.md`](README.md) است.

## ۱. معرفی و معماری

Claude Proxy یک برنامهٔ دسکتاپ ویندوزی است؛ رابط React داخل WebView2 بسته‌بندی می‌شود و کد Rust با Tauri کارهای سیستم‌عامل و پراکسی را انجام می‌دهد. در نسخهٔ ۲٫۰٫۱، ارتباط با مدل بیرونی از API سازگار با OpenAI و مسیر Chat Completions انجام می‌شود. خود Claude Desktop یا Claude CLI از درگاه محلی با قالب Anthropic Messages API صحبت می‌کند.

```text
Claude Desktop / Claude CLI
        │  Anthropic Messages API + توکن محلی
        ▼
127.0.0.1:<port>  ── Axum در bridge_v2.rs
        │  تبدیل Anthropic → OpenAI Chat Completions
        ▼
سرویس مدل کاربر /v1/chat/completions
        │  پاسخ عادی یا SSE
        ▼
تبدیل پاسخ OpenAI → Anthropic و بازگرداندن به Claude
```

رابط وب به‌تنهایی پراکسی را اجرا نمی‌کند. فرمان‌های سمت سیستم از React با `invoke` به فرمان‌های Tauri می‌روند؛ سرویس HTTP محلی هم در همان پردازهٔ Rust اجرا می‌شود. سرور فقط روی `127.0.0.1` گوش می‌دهد و برای استفاده از سیستم‌های دیگر شبکه‌ای طراحی نشده است.

## ۲. فناوری‌ها و پیش‌نیازها

- Windows 10/11، WebView2 Runtime و ابزارهای ساخت C++ برای هدف MSVC.
- Node.js سازگار با Vite 7 و pnpm.
- Rust stable با target ویندوز MSVC؛ نصب Rust و پیش‌نیازهای Tauri باید برای همان معماری سیستم انجام شده باشد.
- دسترسی به اینترنت در اولین نصب وابستگی‌ها، مگر آن‌که cacheهای لازم از قبل موجود باشند.

نسخه‌های قفل‌شده را از `pnpm-lock.yaml` و `src-tauri/Cargo.lock` بخوانید؛ نسخهٔ بازه‌ای کتابخانه‌ها در `package.json` الزاماً نسخهٔ واقعاً نصب‌شده نیست.

## ۳. ساختار مخزن

```text
.
├── src/
│   ├── App.tsx                 صفحه‌ها، فرم‌ها، wizard و هماهنگی وضعیت رابط
│   ├── types.ts                انواع TypeScript و مقدار خالی آغازین
│   ├── main.tsx                نقطهٔ ورود React و بارگذاری Vazirmatn
│   ├── index.css               توکن‌های تم، چیدمان و استایل مؤلفه‌ها
│   ├── lib/tauri.ts            لایهٔ invoke و حالت پیش‌نمایش مرورگر
│   └── assets/                 تصویر برند فعال/غیرفعال رابط
├── src-tauri/
│   ├── src/main.rs             نقطهٔ ورود اجرایی
│   ├── src/lib.rs              واگذاری اجرای برنامه به bridge_v2
│   ├── src/bridge_v2.rs        منطق اصلی Rust، HTTP proxy، تنظیمات و tray
│   ├── icons/                  آیکن‌های بسته‌بندی و آیکن‌های وضعیت زمان اجرا
│   ├── capabilities/           مجوزهای IPC پنجرهٔ Tauri
│   ├── tauri.conf.json         مشخصات برنامه، پنجره، build و assetهای وب
│   ├── Cargo.toml              وابستگی‌های Rust
│   └── Cargo.lock              نسخه‌های دقیق crateها
├── package.json               فرمان‌ها و وابستگی‌های رابط
├── pnpm-lock.yaml             نسخه‌های دقیق بسته‌های JavaScript
├── vite.config.ts             پیکربندی Vite؛ base نسبی برای assetهای وب
├── README.md                  راهنمای استفاده
└── DEVELOPMENT.md             همین راهنمای توسعه
```

`node_modules/`، `dist/` و `src-tauri/target/` خروجی‌های قابل‌بازسازی‌اند و نباید به‌عنوان سورس نگهداری یا تحویل شوند. `.gitignore` آن‌ها را مستثنا می‌کند.

## ۴. نصب، اجرا و build

از ریشهٔ مخزن در PowerShell یا Windows Terminal:

```powershell
corepack enable
pnpm install --frozen-lockfile
pnpm build
pnpm tauri dev
```

`pnpm build` ابتدا TypeScript را type-check می‌کند (`tsc -b`) و سپس خروجی Vite را در `dist/` می‌سازد. `pnpm tauri dev` برنامه را با پنجرهٔ واقعی Tauri اجرا می‌کند. `pnpm dev` فقط preview رابط در مرورگر است؛ در preview، فرمان‌های Rust در دسترس نیستند، سرور محلی بالا نمی‌آید و اعمال تنظیمات Claude انجام نمی‌شود.

برای تست Rust و قالب‌بندی:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

`--offline` را به Cargo اضافه کنید فقط وقتی تمام crateها قبلاً در cache موجودند. اگر build پاک انجام می‌دهید، Cargo باید بتواند وابستگی‌های قفل‌شده را دریافت کند.

ساخت نسخهٔ Release بدون installer:

```powershell
pnpm tauri build --no-bundle
```

پیکربندی Tauri قبل از build فرمان `pnpm build` را اجرا می‌کند و `../dist` را در فایل برنامه embed می‌کند؛ بنابراین exe نهایی برای رابط به dev server روی `localhost:1420` وابسته نیست. فایل اجرایی را در `src-tauri/target/release/claude-proxy.exe` پیدا کنید. `--no-bundle` نصب‌کنندهٔ MSI/NSIS تولید نمی‌کند. WebView2 Runtime همچنان پیش‌نیاز خود ویندوز است.

فرمان‌های جایگزین تعریف‌شده در `package.json` نیز موجودند: `pnpm tauri:dev` و `pnpm tauri:build`.

## ۵. مسیر داده و وضعیت برنامه

### آغاز برنامه

`bridge_v2::run()` مسیر دادهٔ اختصاصی Tauri را می‌گیرد و تنظیمات `bridge-config.json` را از آنجا بار می‌کند. مسیر دقیق را با API مسیرهای Tauri تعیین کنید؛ آن را در کد hard-code نکنید. پیکربندی تازه نام سرویس، نشانی، کلید، فهرست مدل‌ها و نگاشت‌ها را خالی دارد؛ پورت محلی `18181`، تم روشن و بازیابی context روشن است.

وضعیت در حافظه داخل `BridgeState` نگهداری می‌شود. سرور Axum همان وضعیت تنظیمات و همان HTTP client را به اشتراک می‌گذارد. تغییر تنظیمات با `save_config` اعتبارسنجی/نرمال‌سازی می‌شود، به‌صورت اتمی نوشته می‌شود و وضعیت حافظه به‌روز می‌گردد. اگر پورت عوض شود، اجرای پراکسی قبلی متوقف می‌شود؛ برای آدرس جدید کلاینت Claude را دوباره Apply کنید.

### نگهداری رازها

در ویندوز، کلید سرویس و توکن تصادفی درگاه پیش از ذخیره با DPAPI محافظت می‌شوند (`apiKeyProtected` و `gatewayTokenProtected`). فایل تنظیمات برای همان زمینهٔ ویندوزی رمزگشایی می‌شود؛ فایل را به‌عنوان روش انتقال تنظیمات به کاربر یا رایانهٔ دیگر فرض نکنید. برای انتقال تنظیمات، از export/import رمزگذاری‌شده‌ای که آگاهانه طراحی و پیاده‌سازی شده باشد استفاده کنید؛ کپی خام فایل کافی نیست.

هنگام Apply، خود Claude باید توکن localhost را بخواند، بنابراین آن توکن در فایل تنظیمات Claude به‌شکل قابل‌خواندن نوشته می‌شود. از ثبت کلید، headerهای Authorization، توکن درگاه یا بدنهٔ کامل prompt در log، خطا یا telemetry خودداری کنید. پیش از تغییر فایل‌های Claude نسخهٔ پشتیبان با پسوند `.qbs2-backup-<timestamp>` ساخته می‌شود و نوشتن‌ها rollback دارد.

## ۶. قرارداد رابط TypeScript و فرمان‌های Tauri

`src/types.ts` قرارداد سمت رابط را تعریف می‌کند. نوع `ModelRoute` شامل `routeId` (نام قابل‌مشاهده برای Claude)، `displayName`، `upstreamModel` (شناسهٔ واقعی سرویس)، `tier`، `contextWindow`، `maxOutputTokens` و `isDefault` است. هنگام تغییر فیلد، هم‌زمان structهای Rust (`ModelRoute`, `ConfigInput`, `PublicConfig`, `DiskConfig`) و کد serialize/deserialize را بررسی کنید. Rust از `camelCase` در JSON استفاده می‌کند.

`src/lib/tauri.ts` فقط adapter رابط است. نام فرمان‌های ثبت‌شده در `generate_handler![]`:

| فرمان | وظیفه |
| --- | --- |
| `get_config` | خواندن تنظیمات قابل‌نمایش؛ راز خام برنمی‌گردد |
| `save_config` | ذخیرهٔ تنظیمات و کلید در صورت نیاز |
| `get_proxy_status` | وضعیت اجرا، پورت و تعداد routeها |
| `fetch_models` | خواندن فهرست مدل‌های upstream از `/v1/models` |
| `start_proxy` / `stop_proxy` | راه‌اندازی یا توقف listener محلی |
| `test_local_route` | بررسی سلامت listener محلی |
| `apply_claude_desktop` | فعال‌کردن proxy و نوشتن profile کلاینت دسکتاپ |
| `apply_claude_cli` | فعال‌کردن proxy و ادغام env در تنظیمات CLI |
| `open_creator_link` | بازکردن صفحهٔ توسعه‌دهنده |

اگر فرمان تازه‌ای اضافه کردید، سه محل را با هم به‌روزرسانی کنید: تابع `#[tauri::command]` و `generate_handler![]` در Rust، wrapper متناظر در `src/lib/tauri.ts` و رفتار/خطای مناسب در React. قابلیت‌های IPC لازم باید در `src-tauri/capabilities/default.json` مجاز باشند.

## ۷. قرارداد پراکسی و نگاشت مدل

### API محلی

listener پیش‌فرض `127.0.0.1:18181` است و پورت از رابط قابل‌تغییر است. endpointهای پیاده‌شده:

- `GET /healthz` برای سلامت listener (اطلاعات حداقلی؛ بدون پاسخ مدل).
- `GET /v1/models` و مسیرهای Claude Desktop با پیشوند `/claude-desktop`.
- `POST /v1/messages` و `/claude-desktop/v1/messages` برای درخواست Anthropic Messages API.

به‌جز health check، درخواست‌ها باید توکن محلی معتبر را با `Authorization: Bearer ...` یا `x-api-key` بفرستند. فهرست مدل محلی فقط routeهای Claude را منتشر می‌کند؛ نام مدل واقعی Qwen یا مدل دیگر باید در ستون upstream باقی بماند.

### آدرس و احراز هویت upstream

`baseUrl` می‌تواند دامنهٔ ریشه یا نشانی‌ای که به `/v1` ختم می‌شود باشد؛ slash پایانی حذف می‌شود. برنامه در صورت نیاز `/v1` را اضافه می‌کند و برای کشف مدل `GET /models` و برای تولید پاسخ `POST /chat/completions` می‌زند. حالت `bearer` کلید را در Authorization می‌فرستد؛ حالت `api_key` آن را در هدر `api-key` می‌گذارد.

کشف مدل پاسخ‌های JSON دارای آرایهٔ `data` یا `models` را می‌پذیرد؛ هر ردیف می‌تواند رشته، `id` یا `name` باشد. اگر سرویس این endpoint را ندارد، شناسهٔ مدل را دستی وارد کنید. موفقیت کشف `/models` به‌تنهایی سازگاری Chat Completions یا ابزارها را تضمین نمی‌کند.

### شناسه‌های نمایشی Claude

در `src/types.ts` فقط این چهار نقش منتشر می‌شوند: `claude-fable-5-1`، `claude-opus-5-5`، `claude-sonnet-5-5` و `claude-haiku-4-5-20251001`. عنوان‌های رابط کاربر واژهٔ `Claude` را تکرار نمی‌کنند؛ این شناسه‌ها aliasهای قابل‌ارائه به Claude هستند و ادعا نمی‌کنند که upstream واقعاً مدل Anthropic اجرا می‌کند. هنگام بارگذاری تنظیمات قدیمی، alias‌های قبلی بر اساس tier به یکی از این چهار شناسه مهاجرت می‌کنند و نقش‌های حذف‌شده منتشر نمی‌شوند. هر مسیر با شناسهٔ یکتای خودش در `/v1/models` و نمایهٔ Desktop قرار می‌گیرد و هر درخواست بر اساس فیلد `model` همان درخواست resolve می‌شود؛ بنابراین انتخاب مدل یک چت روی چت‌های دیگر اثر نمی‌گذارد.

`anthropic_to_openai()` متن system، پیام‌های user/assistant، تصویرهای پشتیبانی‌شده، tool calls و tool results را به شکل Chat Completions تبدیل می‌کند؛ اگر system prompt وجود داشته باشد، ابتدای آرایهٔ messages قرار می‌گیرد. پاسخ عادی و استریم SSE نیز به قالب Anthropic Messages بازگردانده می‌شود. این مبدل سازگاری کامل با هر توسعهٔ اختصاصی API Anthropic یا هر نوع محتوای چندرسانه‌ای را تضمین نمی‌کند؛ قابلیت تازه باید با نمونهٔ درخواست واقعی و تست واحد سنجیده شود.

## ۸. حد context و حفظ تاریخچه

تنظیم «کنارگذاشتن خودکار نوبت‌های قدیمی» روی خطای قابل‌شناساییِ context overflow، همان درخواست جاری را تا سقف ۶۴ بار با حذف تدریجی قدیمی‌ترین نوبت کامل دوباره می‌فرستد. system prompt و پیام‌های جدیدتر حفظ می‌شوند. این تغییر فقط در payload همان درخواست پراکسی است؛ تاریخچهٔ ذخیره‌شده و قابل‌نمایش در Claude را پاک یا بازنویسی نمی‌کند. با این حال مدل دیگر محتوای نوبت‌هایی را که از payload حذف شده‌اند نمی‌بیند؛ برای ادامهٔ معنایی بحث‌های قدیمی، خلاصهٔ لازم باید در context باقی‌مانده باشد.

`contextWindow` و `maxOutputTokens` در route برای محدودکردن مقدار `max_tokens` خروجی به‌کار می‌روند. برآورد محلی token دقیق نیست؛ محدودیت نهایی را gateway/model اعمال می‌کند. کد بازیابی باید پیام خطاهای provider را تحلیل کند، نه اینکه هر خطای 500 را بی‌قیدوشرط با کوتاه‌کردن تاریخچه retry کند. تست‌های `is_context_overflow` و `trim_oldest_turn` را هنگام تغییر الگوریتم حفظ کنید.

## ۹. پیکربندی Claude Desktop و CLI

- Desktop: `write_desktop_profile()` فایل‌های profile و metadata زیر LocalAppData را به‌روزرسانی می‌کند و deployment mode را برای third-party inference تنظیم می‌کند. داده‌های قبلی JSON را حفظ می‌کند و نوشتن‌ها backup/rollback دارند.
- CLI: `write_cli_settings()` فایل `%USERPROFILE%\.claude\settings.json` را می‌خواند؛ اگر `CLAUDE_CONFIG_DIR` تعریف شده باشد، همان مسیر مبنا قرار می‌گیرد. فقط کلیدهای env موردنیاز proxy/model را ادغام می‌کند و گزینه‌های دیگر را نگه می‌دارد.
- دکمهٔ Apply ابتدا proxy را بالا می‌آورد، سپس تنظیمات کلاینت را می‌نویسد. بعد از Apply، برای Desktop باید برنامه را کامل ببندید و دوباره باز کنید؛ پردازهٔ باز CLI را نیز دوباره اجرا کنید.

اگر فرمت فایل‌های کلاینت با نسخهٔ تازهٔ Claude تغییر کرد، فایل‌های نمونه را از همان نسخهٔ واقعی تهیه کنید، تغییر را محدود و merge-friendly نگه دارید و روی JSON از قبل دارای گزینه‌های دیگر تست کنید. Apply را روی مسیر تصادفی یا فایل بدون backup آزمایش نکنید.

## ۱۰. پنجره، tray و آیکن‌ها

درخواست بستن پنجره لغو و پنجره hide می‌شود تا proxy زنده بماند. کلیک چپ tray پنجره را باز می‌کند؛ منوی راست‌کلیک گزینهٔ خروج کامل دارد. آیکن وضعیت در سه جا بازتاب می‌یابد: تصویر رابط React، آیکن پنجره و tray. هنگام شروع موفق سبز و هنگام توقف نارنجی می‌شود؛ آیکن فایل EXE در زمان نصب نسخهٔ غیرفعال است.

منابع آیکن رابط در `src/assets/proxy-offline.png` و `proxy-active.png` هستند. منابع استاندارد بسته‌بندی Tauri در `src-tauri/icons/` قرار دارند. فایل‌های `.rgba` را Rust با `include_bytes!` در `status_icon()` می‌خواند: دادهٔ خام RGBA ردیفی برای تصویر ۱۲۸×۱۲۸ (۶۵۵۳۶ بایت). اگر تصویر وضعیت عوض شد، هر سه مجموعه را هماهنگ کنید؛ صرفاً PNG را تغییر ندهید و فایل `.rgba` را جا نیندازید. برای ساخت مجموعهٔ آیکن پوسته از CLI، ابتدا `pnpm tauri icon --help` را ببینید و تصویر نارنجی را به‌عنوان آیکن پیش‌فرض وارد کنید؛ سپس اندازه و شفافیت را در Windows بررسی کنید.

## ۱۱. ظاهر، فونت و محتوای embedشده

`src/index.css` منبع اصلی توکن‌های رنگ/فاصله و تم روشن/تاریک است. تم آغازین در `emptyConfig` روشن است و انتخاب کاربر در تنظیمات ذخیره می‌شود. `src/main.tsx` فونت Vazirmatn را از `@fontsource/vazirmatn/arabic.css` وارد می‌کند تا به فونت نصب‌شدهٔ بیرونی وابسته نباشد. مجوز dependency پس از نصب در `node_modules/@fontsource/vazirmatn/LICENSE` قابل مشاهده است.

`vite.config.ts` از `base: "./"` استفاده می‌کند؛ این برای assetهای embedشده در Tauri لازم است. پیش از تغییر آن، build نهایی را از فایل exe آزمایش کنید، چون ممکن است رابط در preview درست باشد اما داخل برنامه با خطای asset/localhost روبه‌رو شود.

## ۱۲. تست‌ها و کنترل کیفیت

تست‌های فعلی Rust در انتهای `bridge_v2.rs` قرار دارند و مواردی چون تنظیم تازهٔ بدون کلید/مدل، بارگذاری آیکن‌ها، نام route، انتقال system prompt به ابتدای payload، تبدیل tool، بازیابی context، شناسایی خطای overflow، تبدیل پاسخ و نرمال‌سازی URL را بررسی می‌کنند.

پیش از تحویل تغییر، حداقل این‌ها را اجرا کنید:

```powershell
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build --no-bundle
```

چک‌لیست دستی پیشنهادی: اجرای clean build؛ شروع و توقف proxy؛ بازدید `http://127.0.0.1:<port>/healthz`؛ آزمون API key اشتباه و درست؛ دریافت `/v1/models`؛ یک درخواست ساده، استریم، tool-use و پاسخ upstream با خطای context؛ تغییر پورت؛ Apply روی CLI و Desktop؛ حفظ کلیدهای نامرتبط در JSONهای قبلی؛ close-to-tray، بازکردن پنجره از tray و خروج کامل.

## ۱۳. راهنمای عیب‌یابی

| نشانه | مسیر بررسی |
| --- | --- |
| پنجره باز نمی‌شود یا `localhost refused` می‌دهد | برای نسخهٔ توسعه `pnpm tauri dev` را اجرا کنید؛ برای نسخهٔ Release، `dist/` را قبل از Tauri build بسازید و `base: "./"` را نگه دارید. exe نهایی را از مسیر release اجرا کنید، نه یک dev URL قدیمی. |
| پیام «پراکسی پاسخ نداد» | وضعیت listener و پورت را ببینید، `healthz` را باز کنید، مطمئن شوید پورت اشغال نیست و سرویس روی loopback bind شده است. پس از تغییر پورت، دوباره Apply کنید. |
| کشف مدل خالی/ناموفق است | base URL، هدر احراز هویت و پاسخ `/v1/models` را کنترل کنید. در صورت نبود endpoint، مدل را دستی اضافه کنید. |
| مدل Claude را قبول نمی‌کند | route سمت Claude باید alias با prefix مجاز باشد؛ نام Qwen واقعی را در upstream model قرار دهید و نقش را دوباره Apply کنید. |
| خطای system باید ابتدای پیام باشد | به `anthropic_to_openai()` نگاه کنید؛ system باید قبل از کل پیام‌های دیگر قرار گیرد. تست `system_prompt_is_first_and_model_name_is_remapped` مانع regression است. |
| context overflow | context window و سقف خروجی route را بررسی کنید. بازیابی خودکار فقط روی خطاهای شناخته‌شده retry می‌کند و history داخل Claude را حذف نمی‌کند. |
| آیکن یکپارچه نیست | PNG رابط، آیکن استاندارد bundle، دو فایل raw `.rgba` و مسیرهای `include_bytes!` را با هم بررسی کنید؛ سپس clean Rust rebuild کنید اگر فایل embedشده به‌روز نشده است. |
| تنظیم Claude اثر نکرد | محتوای JSON و backup کناری را بررسی کنید؛ Claude Desktop/CLI را کامل ببندید و باز کنید؛ وجود چند نصب/پروفایل یا `CLAUDE_CONFIG_DIR` را کنترل کنید. |

هنگام گزارش خطا، نسخه، فرمان اجرا، مرحلهٔ بازتولید و متن پاسخ HTTP را ثبت کنید؛ کلید API، bearer token، Authorization header و متن خصوصی گفتگو را حذف/ماسک کنید.

## ۱۴. انتشار و تغییر نسخه

نسخه در `package.json`، `src-tauri/Cargo.toml` و `src-tauri/tauri.conf.json` تعریف می‌شود؛ هنگام bump، نسخهٔ package در `src-tauri/Cargo.lock` را نیز با `cargo check` یا فرمان build به‌روز و diff را بررسی کنید. ترتیب پیشنهادی:

1. تغییرات را در واحدهای کوچک انجام دهید و برای رفتار پروتکل Rust تست بنویسید.
2. هر چهار کنترل کیفیت بخش قبل را اجرا کنید.
3. نسخه‌ها و نام فایل خروجی را هماهنگ کنید.
4. فایل exe خروجی، اندازه/نسخهٔ فایل و اجرای واقعی آن روی Windows را بررسی کنید.
5. README، این راهنما و یادداشت تغییرات را با رفتار واقعی نسخه هماهنگ کنید.

## ۱۵. اصولی که در توسعه حفظ شوند

- listener فقط روی loopback بماند، مگر اینکه احراز هویت، threat model و پیامد دسترسی شبکه‌ای جداگانه طراحی شده باشند.
- secret را به React state موقت محدود کنید؛ از ذخیره در `localStorage`، log، error toast یا فایل نمونه پرهیز کنید.
- تنظیمات کاربر و فایل Claude را با merge، backup و نوشتن اتمی تغییر دهید؛ کل فایل را بی‌دلیل جایگزین نکنید.
- قرارداد TypeScript و Rust را هم‌زمان تغییر دهید و JSON قدیمی را با `serde(default)` تا حد ممکن قابل‌خواندن نگه دارید.
- برای upstream retry محدود و بر اساس نوع خطا بنویسید؛ خطاهای auth یا درخواست نامعتبر را retry نکنید.
- هنگام کوچک‌کردن context، تفاوت بین history ذخیره‌شده در Claude و payload ارسالی به مدل را در UI/مستندات روشن نگه دارید.
- dependency یا asset خارجی را به رابط اضافه نکنید، مگر اینکه در نسخهٔ packaged و با محدودیت شبکه نیز بررسی شده باشد.



## نسخه‌گذاری اجباری برای هر بستهٔ تغییرات

نسخهٔ قابل‌نمایش کاربر باید برای **هر بستهٔ اصلاحات/قابلیت که Portable تازه می‌سازد** افزایش یابد. برای اصلاح معمولی مقدار patch یک واحد زیاد می‌شود (`2.0.10` → `2.0.11`)؛ با تغییرات ناسازگار، افزایش minor یا major آگاهانه انجام شود. بازساخت کد با همان شمارهٔ نسخه برای کاربران ممنوع است. یک PR در حالت Draft می‌تواند چند commit آماده‌سازی داشته باشد اما پیش از تحویل فایل به کاربر، شمارهٔ نسخهٔ منتشرشده باید یکتا باشد.

از ریشهٔ مخزن و پیش از کدزدن هر بستهٔ جدید:

```powershell
node scripts/bump-version.mjs
node scripts/check-version.mjs
```

این فرمان همهٔ نقاط ثبت نسخه (`package.json`، `src-tauri/tauri.conf.json`، `src-tauri/Cargo.toml`، `src-tauri/Cargo.lock` و عنوان `README.md`) را با هم تغییر می‌دهد. نسخهٔ رابط از `package.json` خوانده می‌شود؛ نسخهٔ قابل مشاهده در Sidebar و Topbar یکسان است. نام EXE و artifact اکشن ویندوز `Claude-Proxy-vX.Y.Z-Portable.exe` و `Claude-Proxy-vX.Y.Z-Portable` است. اطلاعات رمزنگاری‌شده، پروفایل Claude و تنظیمات ذخیره‌شدهٔ کاربر با افزایش شمارهٔ نسخه حذف نمی‌شوند.

Workflow بررسی V2 ناسازگاری نسخه‌ها را رد می‌کند و در Pull Request نسخهٔ جدید را با نسخهٔ پایه مقایسه می‌کند؛ اگر bump نکرده باشد، CI موفق نمی‌شود. نسخهٔ تأییدشده فقط زمانی قابل معرفی است که Rust tests، Frontend و Portable روی **یک SHA** موفق باشند. برای هر PR تغییر بعدی ابتدا نسخه را مجدداً افزایش بدهید؛ شمارهٔ یک بستهٔ آزمایشی قبلی را تکرار نکنید.


## 2.0.11 — سازگاری نقش‌های پیام Cowork و نسخه‌گذاری

خطای `API Error: 400 Unsupported message role in client request` از اعتبارسنجی سخت‌گیرانهٔ PR #26 می‌آمد: آن کد فقط `user` و `assistant` را می‌پذیرفت. در این نسخه `system` و `developer` با نقش کنترلی خودشان حفظ می‌شوند؛ `tool`/ `tool_result` تنها با شناسهٔ واقعی `tool_call_id` یا `tool_use_id` به پیام ابزار OpenAI تبدیل می‌شوند. تبدیل ناشناخته به `user`، ساختن شناسهٔ ابزار جعلی و حذف ساکت محتوای نامعتبر مجاز نیست. خطاهای باقیمانده اکنون کد `COWORK_UNSUPPORTED_MESSAGE_ROLE`، شمارهٔ پیام و نام امن نقش را گزارش می‌کنند، بدون ذخیرهٔ محتوای مکالمه یا توکن.

**نسخه:** `2.0.11`. نام فایل `Claude-Proxy-v2.0.11-Portable.exe` و نام آرتیفکت GitHub `Claude-Proxy-v2.0.11-Portable` خواهد بود. شماره در نوار بالا و سایدبار نیز دیده می‌شود. افزایش نسخه در بستهٔ تغییر بعدی الزامی است و باید با `node scripts/bump-version.mjs` و `node scripts/check-version.mjs` در `package.json`، Tauri، Cargo، Cargo.lock و README هماهنگ شود. PR CI این موضوع را در برابر نسخهٔ پایه کنترل می‌کند. این اصلاح برچسب نسخه/آداپتر است و به‌تنهایی کیفیت پاسخ Qwen را تضمین نمی‌کند.


## نسخهٔ 2.0.12 — تست واقعی قابلیت وب

در صفحهٔ Advanced → Web access، دکمهٔ «تست واقعی وب و GitHub» سه مسیر را به‌طور مستقل آزمایش می‌کند: `/healthz` محلی بدون پروکسی و توکن؛ موتور جست‌وجوی تنظیم‌شده با همان Web Proxy ذخیره‌شده؛ و خواندن صفحهٔ عمومی `https://github.com/thedotmack/claude-mem`. پاسخ HTTP 200 بدون نتایج قابل‌استخراج یا بدون نام مخزن، موفقیت محسوب نمی‌شود. گزارش فقط `ok`، نوع تست، علت دسته‌بندی‌شدهٔ شکست و زمان پاسخ را بازمی‌گرداند و محتوا یا کلیدها ذخیره نمی‌شوند. این درخواست‌ها فقط با کلیک کاربر اجرا می‌شوند.

برای سازگاری با نسخه‌های فعلی، ابزارهای Native متعلق به Cowork/Code همچنان اولویت دارند؛ این تست درستی دسترسی وب داخل Cowork یا سیاست مجوز سازمانی را تضمین نمی‌کند و راهی برای دورزدن آن نیست. محدودیت DNS محلی هنگام استفاده از پروکسی در تابع `validate_resolved_web_host` باید جداگانه با ملاحظات SSRF بررسی شود. اجرای تست نیازمند ذخیرهٔ تنظیمات و فعال‌بودن Web Access است. آزمون رگرسیون طبقه‌بندی، پاسخ خالی/مسدود و محتوای واقعی GitHub در Rust افزوده شد.
