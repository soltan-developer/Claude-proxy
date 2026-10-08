export type Language = "en" | "fa";

const latinDigits: Record<string, string> = {
  "۰": "0", "۱": "1", "۲": "2", "۳": "3", "۴": "4",
  "۵": "5", "۶": "6", "۷": "7", "۸": "8", "۹": "9",
  "٠": "0", "١": "1", "٢": "2", "٣": "3", "٤": "4",
  "٥": "5", "٦": "6", "٧": "7", "٨": "8", "٩": "9",
};

const fa: Record<string, string> = {
  "Loading Claude Proxy…": "در حال بارگذاری کلاد پروکسی…",
  "Local gateway for Claude": "درگاه محلی برای کلاد",
  "Application sections": "بخش‌های برنامه",
  "Privacy & usage rights": "حریم خصوصی و حقوق استفاده",
  "Everything runs on this device.": "حریم خصوصی و حقوق استفاده",
  "Version": "نسخه",
  "Created by Salmanzadeh": "ساخته‌شده توسط سلمان زاده",
  "Toggle light or dark mode": "تغییر پوستهٔ روشن یا تیره",
  "Switch to Persian": "تغییر زبان به فارسی",
  "Switch to English": "تغییر زبان به انگلیسی",
  "Minimize": "کوچک‌کردن",
  "Maximize or restore": "بزرگ‌کردن یا بازگردانی",
  "Close window": "بستن پنجره",
  "Dismiss": "بستن پیام",
  "Dismiss error": "بستن پیام خطا",
  "Overview": "نمای کلی",
  "Token usage": "مصرف توکن",
  "Provider connection": "اتصال به ارائه‌دهنده",
  "Model routes": "مسیرهای مدل",
  "Claude clients": "کلاینت‌های کلاد",
  "API pool": "استخر API",
  "Advanced settings": "تنظیمات پیشرفته",
  "Diagnostics": "گزارش خطاها",
  "ADVANCED SETTINGS": "تنظیمات پیشرفته",
  "Claude output limit": "محدودیت خروجی کلاد",
  "Set the maximum output token value for future Claude sessions.": "حداکثر تعداد توکن خروجی را برای نشست‌های بعدی کلاد تنظیم کنید.",
  "Maximum output tokens": "حداکثر توکن خروجی",
  "Range: 1–128,000. Current setting: CLAUDE_CODE_MAX_OUTPUT_TOKENS": "بازه: ۱ تا ۱۲۸٬۰۰۰. متغیر تنظیم‌شده: CLAUDE_CODE_MAX_OUTPUT_TOKENS",
  "Applying this writes the value to your Windows user environment. Fully quit and reopen Claude Desktop for new sessions to inherit it. This variable is documented for Claude Code; Cowork may not honor it, and provider or model limits still apply.": "با اعمال این گزینه، مقدار در متغیرهای محیطی حساب کاربری ویندوز ذخیره می‌شود. برای دریافت آن در نشست‌های جدید، کلاد دسکتاپ را کاملاً ببندید و دوباره باز کنید. این متغیر برای Claude Code مستند شده است؛ ممکن است Cowork از آن پیروی نکند و محدودیت ارائه‌دهنده یا مدل نیز همچنان اعمال می‌شود.",
  "Save and apply to Claude Desktop": "ذخیره و اعمال برای کلاد دسکتاپ",
  "Applying…": "در حال اعمال…",
  "Applying the Windows environment setting is available in the installed desktop app.": "اعمال متغیر محیطی ویندوز در نسخهٔ نصب‌شدهٔ برنامه در دسترس است.",
  "Web access bridge": "پل دسترسی وب",
  "Let Claude use a controlled web tool that runs on this computer, while the model API stays on your configured provider.": "به کلاد اجازه دهید از ابزار وب کنترل‌شده‌ای استفاده کند که روی همین رایانه اجرا می‌شود؛ API مدل همچنان از ارائه‌دهندهٔ تنظیم‌شدهٔ شما استفاده می‌کند.",
  "Web access": "دسترسی وب",
  "The bridge only runs when you enable it here.": "پل وب فقط زمانی اجرا می‌شود که آن را از اینجا فعال کنید.",
  "Web access mode": "حالت دسترسی وب",
  "Disabled": "خاموش",
  "Automatic — model decides": "خودکار — تصمیم با مدل",
  "Required — search first": "اجباری — ابتدا جست‌وجو شود",
  "Web traffic route": "مسیر ترافیک وب",
  "Direct connection": "اتصال مستقیم",
  "Manual proxy": "پروکسی دستی",
  "Web proxy address": "نشانی پروکسی وب",
  "HTTP / SOCKS5": "HTTP / SOCKS5",
  "For example: 127.0.0.1:10808": "برای نمونه: 127.0.0.1:10808",
  "Web search endpoint": "نشانی جست‌وجوی وب",
  "An HTTPS search endpoint returning DuckDuckGo HTML results. Keep the default unless your network requires another endpoint.": "نشانی HTTPS برای دریافت نتایج HTML داک‌داک‌گو است. مگر اینکه شبکهٔ شما نشانی دیگری بخواهد، مقدار پیش‌فرض را نگه دارید.",
  "Preferred web domains": "دامنه‌های پیشنهادی وب",
  "One hostname per line. These are source preferences for the model, not a network allowlist. A URL you provide is fetched directly without a search first.": "در هر خط یک نام دامنه وارد کنید. این فهرست فقط منبع‌های پیشنهادی برای مدل است و مجوز شبکه ایجاد نمی‌کند. نشانی‌ای که خودتان می‌دهید بدون جست‌وجوی قبلی مستقیماً دریافت می‌شود.",
  "Web access is required": "دسترسی وب اجباری است",
  "Web access is automatic": "دسترسی وب خودکار است",
  "Web access is disabled": "دسترسی وب خاموش است",
  "Traffic will use a direct connection.": "ترافیک از اتصال مستقیم عبور می‌کند.",
  "Traffic will use the configured proxy.": "ترافیک از پروکسی تنظیم‌شده عبور می‌کند.",
  "Save advanced settings": "ذخیرهٔ تنظیمات پیشرفته",
  "Test web connection": "آزمایش اتصال وب",
  "Testing web access…": "در حال آزمایش دسترسی وب…",
  "Web access is working through direct connection.": "دسترسی وب از طریق اتصال مستقیم فعال است.",
  "Enable Automatic or Required web access before testing the web bridge.": "پیش از آزمایش پل وب، حالت خودکار یا اجباری دسترسی وب را فعال کنید.",
  "Live web checks are available in the installed Windows app.": "آزمایش زندهٔ وب در برنامهٔ نصب‌شدهٔ ویندوز در دسترس است.",
  "HOW IT WORKS": "روش کار",
  "One setting, two routes": "یک تنظیم، دو مسیر",
  "Automatic mode does not force web access for ordinary messages. It searches when a GitHub repository is named, reuses fetched context for follow-up opinions, and fetches again when you ask for code changes. Required mode searches each request except a URL is fetched directly. Web requests run through this computer and do not change Cowork's administrator network allowlist.": "در حالت خودکار، برای پیام‌های عادی دسترسی وب اجباری نمی‌شود. با نام بردن از مخزن گیت‌هاب جست‌وجو می‌کند، برای پرسش‌های بعدی از زمینهٔ دریافت‌شده استفاده می‌کند و هنگام درخواست تغییر کد دوباره مخزن را می‌خواند. حالت اجباری در هر درخواست جست‌وجو می‌کند؛ مگر اینکه نشانی داده باشید که مستقیم دریافت می‌شود. درخواست وب از این رایانه می‌گذرد و فهرست مجاز شبکهٔ مدیر Cowork را تغییر نمی‌دهد.",
  "Model API": "API مدل",
  "Uses your provider URL and API key.": "از نشانی و کلید API ارائه‌دهندهٔ شما استفاده می‌کند.",
  "Web bridge": "پل وب",
  "Uses Direct or the proxy address above.": "از اتصال مستقیم یا نشانی پروکسی بالا استفاده می‌کند.",
  "Only public HTTP(S) pages are allowed. Localhost, private IPs, metadata services, and credentials in URLs are blocked before a request is sent.": "فقط صفحه‌های عمومی HTTP(S) مجاز هستند. لوکال‌هاست، IPهای خصوصی، سرویس‌های فراداده و نشانی‌های دارای اطلاعات ورود پیش از ارسال درخواست مسدود می‌شوند.",
  "Setup": "راه‌اندازی",
  "ADVANCED ROUTING": "مسیریابی پیشرفته",
  "Bring multiple provider accounts together behind one Claude model route.": "چند حساب ارائه‌دهنده را پشت یک مسیر مدل کلاد گرد هم بیاورید.",
  "Special access · Coming in a future release": "دسترسی ویژه · در نسخه‌ای آینده",
  "One model name, multiple provider routes": "یک نام مدل، چند مسیر ارائه‌دهنده",
  "The API pool will let you register providers and keys, inspect their available models, then create an internal model backed by several provider models. Requests can be distributed by random selection, ordered failover, weighted routing, or other strategies.": "استخر API به شما امکان می‌دهد چند ارائه‌دهنده و کلید ثبت کنید، مدل‌های در دسترسشان را ببینید و یک مدل داخلی بسازید که چند مدل ارائه‌دهنده پشت آن قرار دارند. درخواست‌ها می‌توانند به‌صورت تصادفی، با ترتیب پشتیبان‌گیری، بر اساس وزن یا با روش‌های دیگر توزیع شوند.",
  "PROVIDERS": "ارائه‌دهندگان",
  "Provider accounts": "حساب‌های ارائه‌دهنده",
  "Preview only": "فقط پیش‌نمایش",
  "Endpoint and protected key": "نشانی و کلید محافظت‌شده",
  "Not connected": "متصل نیست",
  "OpenAI-compatible gateway": "درگاه سازگار با OpenAI",
  "Local fallback": "پشتیبان محلی",
  "2 models · Protected API key": "۲ مدل · کلید API محافظت‌شده",
  "1 model · Protected API key": "۱ مدل · کلید API محافظت‌شده",
  "Ready": "آماده",
  "Accounts and keys would be managed here in a future release.": "در نسخه‌ای آینده، حساب‌ها و کلیدها از همین بخش مدیریت خواهند شد.",
  "Add provider account": "افزودن حساب ارائه‌دهنده",
  "INTERNAL MODEL": "مدل داخلی",
  "Route composition": "ترکیب مسیرها",
  "Unavailable": "در دسترس نیست",
  "Internal model name": "نام مدل داخلی",
  "For example, balanced-assistant": "برای نمونه: balanced-assistant",
  "Provider model A": "مدل ارائه‌دهندهٔ الف",
  "Provider model B": "مدل ارائه‌دهندهٔ ب",
  "Primary route": "مسیر اصلی",
  "Fallback route": "مسیر پشتیبان",
  "Weighted route": "مسیر وزن‌دار",
  "Qwen3-32B": "Qwen3-32B",
  "Llama-3.3-70B-Instruct": "Llama-3.3-70B-Instruct",
  "DeepSeek-V3": "DeepSeek-V3",
  "Mistral-Large": "Mistral-Large",
  "Request strategy": "روش توزیع درخواست",
  "Ordered failover": "جابه‌جایی پشتیبان با ترتیب مشخص",
  "Random": "تصادفی",
  "Weighted": "وزن‌دار",
  "Create internal model": "ساخت مدل داخلی",
  "Read the full non-commercial license": "مطالعهٔ متن کامل مجوز غیرتجاری",
  "CLAUDE DESKTOP": "کلاد دسکتاپ",
  "This screen is a visual mockup only. It does not store provider keys, connect to provider APIs, or send requests.": "این صفحه فقط یک پیش‌نمایش ظاهری است؛ کلیدها را ذخیره نمی‌کند، به API ارائه‌دهندگان وصل نمی‌شود و درخواستی نمی‌فرستد.",
  "LOCAL AI GATEWAY": "درگاه محلی هوش مصنوعی",
  "Claude, with your own models.": "کلاد، با مدل‌های خودتان.",
  "A simple local route between Claude and your model provider.": "مسیری ساده و محلی میان کلاد و ارائه‌دهندهٔ مدل شما.",
  "Claude requests are routed from the local gateway to your provider.": "درخواست‌های کلاد از درگاه محلی به ارائه‌دهندهٔ شما هدایت می‌شوند.",
  "Setup guide": "راهنمای راه‌اندازی",
  "Setup is not complete yet": "راه‌اندازی هنوز کامل نشده است",
  "Add your provider URL, map your models, and connect Claude.": "نشانی ارائه‌دهنده را وارد کنید، مدل‌ها را نگاشت کنید و کلاد را متصل کنید.",
  "Start setup": "شروع راه‌اندازی",
  "PROXY STATUS": "وضعیت پروکسی",
  "Local endpoint": "نشانی محلی",
  "Copy address": "کپی نشانی",
  "Port can be changed in Provider connection": "در بخش اتصال به ارائه‌دهنده می‌توانید درگاه را تغییر دهید",
  "Provider settings": "تنظیمات ارائه‌دهنده",
  "Manage routes →": "مدیریت مسیرها ←",
  "Clients": "کلاینت‌ها",
  "Desktop and CLI": "دسکتاپ و خط فرمان",
  "Connect a client →": "اتصال کلاینت ←",
  "CONFIGURATION": "پیکربندی",
  "Quick overview": "نمای سریع",
  "Edit settings →": "ویرایش تنظیمات ←",
  "Model provider": "ارائه‌دهندهٔ مدل",
  "Default model": "مدل پیش‌فرض",
  "Connect Claude": "اتصال کلاد",
  "Claude Desktop or Claude CLI": "کلاد دسکتاپ یا خط فرمان کلاد",
  "Your settings are ready. Start the proxy or apply them from Claude clients.": "تنظیمات آماده است. پروکسی را اجرا کنید یا آن‌ها را از بخش کلاینت‌های کلاد اعمال کنید.",
  "Connect a provider and add at least one model route to get started.": "برای شروع، به یک ارائه‌دهنده وصل شوید و دست‌کم یک مسیر مدل اضافه کنید.",
  "Connected": "متصل",
  "Running": "در حال اجرا",
  "Stopped": "متوقف",
  "Waiting for a connection": "در انتظار اتصال",
  "PROVIDER CONNECTION": "اتصال به ارائه‌دهنده",
  "Configure provider": "پیکربندی ارائه‌دهنده",
  "Enter an OpenAI-compatible API URL and authentication details.": "نشانی API سازگار با OpenAI و اطلاعات احراز هویت را وارد کنید.",
  "Provider details": "جزئیات ارائه‌دهنده",
  "These values are stored on this device.": "این اطلاعات روی همین دستگاه ذخیره می‌شوند.",
  "Provider name": "نام ارائه‌دهنده",
  "Optional": "اختیاری",
  "For example, internal gateway": "برای نمونه: درگاه داخلی",
  "Base API URL": "نشانی پایهٔ API",
  "You can include or omit /v1; Claude Proxy detects it automatically.": "می‌توانید ‎/v1 را وارد کنید یا نکنید؛ کلاد پروکسی آن را خودکار تشخیص می‌دهد.",
  "API key": "کلید API",
  "Saved and protected": "ذخیره و محافظت شده",
  "Saved API key is protected": "کلید API ذخیره‌شده محافظت شده است",
  "Enter a replacement API key": "کلید جایگزین API را وارد کنید",
  "Leave blank to keep the saved key": "برای نگه‌داشتن کلید ذخیره‌شده، این بخش را خالی بگذارید",
  "Provider API key": "کلید API ارائه‌دهنده",
  "The key is protected with Windows account encryption and is never shown again in the interface.": "کلید با رمزنگاری حساب ویندوز محافظت می‌شود و دوباره در رابط کاربری نمایش داده نخواهد شد.",
  "Keep saved key": "نگه‌داشتن کلید ذخیره‌شده",
  "Remove saved key": "حذف کلید ذخیره‌شده",
  "Show API key": "نمایش کلید API",
  "Hide API key": "پنهان‌کردن کلید API",
  "Show": "نمایش",
  "Hide": "پنهان‌کردن",
  "Remove the saved provider API key? You will need to enter it again before making provider requests.": "کلید API ذخیره‌شدهٔ ارائه‌دهنده حذف شود؟ پیش از ارسال درخواست‌های بعدی باید آن را دوباره وارد کنید.",
  "Authentication": "احراز هویت",
  "Bearer token": "توکن Bearer",
  "api-key header": "سربرگ api-key",
  "Local port": "درگاه محلی",
  "The default is 18181. Choose another available port if it is in use. Apply the Claude settings again after changing it.": "درگاه پیش‌فرض ۱۸۱۸۱ است. اگر اشغال بود، درگاه آزاد دیگری انتخاب کنید. پس از تغییر، تنظیمات کلاد را دوباره اعمال کنید.",
  "Ping checks the provider’s /v1/models endpoint. Some providers may not expose this endpoint.": "آزمون پینگ، نشانی ‎/v1/models ارائه‌دهنده را بررسی می‌کند. ممکن است بعضی ارائه‌دهندگان این نشانی را در دسترس نگذارند.",
  "Live connections are unavailable in the browser preview. Use the Windows app.": "اتصال زنده در پیش‌نمایش مرورگر در دسترس نیست؛ از برنامهٔ ویندوز استفاده کنید.",
  "Continue when the context limit is reached": "ادامهٔ گفتگو پس از رسیدن به سقف متن زمینه",
  "For long conversations": "برای گفتگوهای طولانی",
  "Automatically fit long conversations": "تنظیم خودکار گفتگوهای طولانی",
  "Before sending, Claude Proxy estimates the context budget and removes the oldest complete turns when needed. It retries only if the provider still reports an overflow.": "کلاد پروکسی پیش از ارسال، بودجهٔ متن زمینه را برآورد می‌کند و در صورت نیاز کامل‌ترین بخش‌های قدیمی گفتگو را کنار می‌گذارد. فقط اگر ارائه‌دهنده همچنان خطای عبور از سقف بدهد، دوباره تلاش می‌کند.",
  "Claude's visible history is not changed. Older messages are omitted only from future requests to the model. Include a summary when older details are needed.": "تاریخچهٔ قابل‌مشاهدهٔ کلاد تغییر نمی‌کند. پیام‌های قدیمی فقط از درخواست‌های بعدی به مدل کنار گذاشته می‌شوند. اگر جزئیات قبلی لازم است، خلاصه‌ای از آن‌ها بنویسید.",
  "Local proxy port": "درگاه پروکسی محلی",
  "Available only on this computer.": "فقط روی همین رایانه در دسترس است.",
  "Test local proxy": "آزمون پروکسی محلی",
  "Use the proxy status control in the sidebar to start or stop the local gateway.": "برای اجرای درگاه محلی یا توقف آن، از کنترل وضعیت پروکسی در نوار کناری استفاده کنید.",
  "MODEL ROUTES": "مسیرهای مدل",
  "Map each Claude role to your model": "هر نقش کلاد را به مدل دلخواه نگاشت کنید",
  "Claude keeps its role names. Map each role to the model ID used by your provider.": "نام نقش‌های کلاد حفظ می‌شود. هر نقش را به شناسهٔ مدلی که ارائه‌دهنده دارد نگاشت کنید.",
  "Claude role → provider model": "نقش کلاد ← مدل ارائه‌دهنده",
  "For example, map the Fable role to any model ID available in your gateway.": "برای نمونه، نقش Fable را به شناسهٔ هر مدلی که در درگاه شما موجود است نگاشت کنید.",
  "Default": "پیش‌فرض",
  "Remove route": "حذف مسیر",
  "Provider model ID": "شناسهٔ مدل ارائه‌دهنده",
  "Actual model ID": "شناسهٔ واقعی مدل",
  "Display name in Claude": "نام نمایشی در کلاد",
  "Context limit": "سقف متن زمینه",
  "tokens": "توکن",
  "For example, 200000": "برای نمونه: ۲۰۰۰۰۰",
  "Maximum output": "حداکثر خروجی",
  "0 = no local limit": "۰ = بدون محدودیت محلی",
  "No model routes yet": "هنوز مسیری برای مدل تعریف نشده است",
  "Choose a Claude role and enter the exact model ID from your provider.": "یک نقش کلاد انتخاب کنید و شناسهٔ دقیق مدل ارائه‌دهنده را وارد کنید.",
  "NEW ROUTE": "مسیر جدید",
  "Add a role": "افزودن نقش",
  "Claude role": "نقش کلاد",
  "Choose a role": "انتخاب نقش",
  "Provider model": "مدل ارائه‌دهنده",
  "For example, northstar-reasoner": "برای نمونه: northstar-reasoner",
  "Add route": "افزودن مسیر",
  "Suggested context limits are 128,000 for qwen38 and 200,000 for qwen38-sglang. Adjust these values if your provider differs.": "سقف متن زمینهٔ پیشنهادی برای qwen38 برابر ۱۲۸٬۰۰۰ و برای qwen38-sglang برابر ۲۰۰٬۰۰۰ است. اگر ارائه‌دهندهٔ شما مقادیر دیگری دارد، آن‌ها را تغییر دهید.",
  "CLIENT SETUP": "راه‌اندازی کلاینت",
  "Claude Proxy writes the required settings directly to each client.": "کلاد پروکسی تنظیمات لازم را مستقیماً برای هر کلاینت می‌نویسد.",
  "LOCAL PROXY ADDRESS": "نشانی پروکسی محلی",
  "The proxy is running": "پروکسی در حال اجراست",
  "Applying client settings also starts the proxy": "با اعمال تنظیمات کلاینت، پروکسی هم اجرا می‌شود",
  "Claude Desktop": "کلاد دسکتاپ",
  "Writes the model routes and local gateway to the inference profile. Fully quit and reopen Claude Desktop after applying the settings.": "مسیرهای مدل و درگاه محلی را در نمایهٔ اجرا می‌نویسد. پس از اعمال تنظیمات، کلاد دسکتاپ را کاملاً ببندید و دوباره باز کنید.",
  "Configuration method": "روش پیکربندی",
  "Gateway profile": "نمایهٔ درگاه",
  "Configure Claude Desktop": "پیکربندی کلاد دسکتاپ",
  "CLAUDE CODE": "کلاد کد",
  "Claude Code": "کلاد کد",
  "Writes the required ANTHROPIC variables to your user settings. Close and reopen any active Claude Code sessions.": "متغیرهای لازم ANTHROPIC را در تنظیمات کاربری می‌نویسد. نشست‌های فعال کلاد کد را ببندید و دوباره باز کنید.",
  "User settings": "تنظیمات کاربر",
  "Configure Claude Code": "پیکربندی کلاد کد",
  "Map at least one Claude role to a provider model in Model routes first.": "ابتدا در بخش مسیرهای مدل، دست‌کم یک نقش کلاد را به مدل ارائه‌دهنده نگاشت کنید.",
  "Applying settings to Claude is available in the installed Windows app.": "اعمال تنظیمات کلاد در نسخهٔ نصب‌شدهٔ ویندوز در دسترس است.",
  "The provider API key is encrypted by the app. Only the local token Claude needs is written to that client's settings file.": "کلید API ارائه‌دهنده در برنامه رمزگذاری می‌شود. فقط توکن محلی موردنیاز کلاد در فایل تنظیمات همان کلاینت نوشته می‌شود.",
  "QUICK SETUP": "راه‌اندازی سریع",
  "Skip for now": "فعلاً رد کن",
  "STEP 1": "گام ۱",
  "Connect to your model provider": "اتصال به ارائه‌دهندهٔ مدل",
  "Enter an OpenAI-compatible URL and access key. No provider or key is preconfigured.": "نشانی سازگار با OpenAI و کلید دسترسی را وارد کنید. هیچ ارائه‌دهنده یا کلیدی از پیش تنظیم نشده است.",
  "Provider label": "عنوان ارائه‌دهنده",
  "For example, company gateway": "برای نمونه: درگاه شرکت",
  "STEP 2": "گام ۲",
  "Find available models": "یافتن مدل‌های در دسترس",
  "Claude Proxy reads models from /v1/models. If your provider does not offer that endpoint, enter model IDs manually later.": "کلاد پروکسی فهرست مدل‌ها را از ‎/v1/models می‌خواند. اگر ارائه‌دهندهٔ شما این نشانی را ندارد، بعداً شناسهٔ مدل‌ها را دستی وارد کنید.",
  "The model list has not been fetched yet": "فهرست مدل‌ها هنوز دریافت نشده است",
  "Fetching models is optional.": "دریافت فهرست مدل‌ها اختیاری است.",
  "Live model discovery is available in the desktop app. You can skip this step.": "یافتن زندهٔ مدل‌ها در برنامهٔ دسکتاپ در دسترس است. می‌توانید از این گام بگذرید.",
  "STEP 3": "گام ۳",
  "Map a Claude role to a model": "نگاشت یک نقش کلاد به مدل",
  "Claude clients use role IDs. Choose the provider model for each role here.": "کلاینت‌های کلاد از شناسهٔ نقش‌ها استفاده می‌کنند. مدل ارائه‌دهندهٔ هر نقش را اینجا انتخاب کنید.",
  "You can skip this step and add model routes later.": "می‌توانید از این گام بگذرید و بعداً مسیرهای مدل را اضافه کنید.",
  "STEP 4": "گام ۴",
  "You're ready to connect": "همه‌چیز برای اتصال آماده است",
  "Your settings will be saved. Configure Claude Desktop or Claude Code separately from the Claude clients page.": "تنظیمات شما ذخیره می‌شود. کلاد دسکتاپ یا کلاد کد را جداگانه از صفحهٔ کلاینت‌های کلاد پیکربندی کنید.",
  "Provider": "ارائه‌دهنده",
  "Not configured yet": "هنوز پیکربندی نشده است",
  "I'll do this later": "بعداً انجام می‌دهم",
  "After setup, configure Claude Desktop or Claude Code from the Claude clients page.": "پس از راه‌اندازی، کلاد دسکتاپ یا کلاد کد را از صفحهٔ کلاینت‌های کلاد پیکربندی کنید.",
  "Configure manually": "پیکربندی دستی",
  "Previous": "قبلی",
  "You can revisit provider, model, and client settings at any time.": "هر زمان بخواهید می‌توانید به تنظیمات ارائه‌دهنده، مدل و کلاینت برگردید.",
  "Privacy notice": "اعلان حریم خصوصی",
  "Purpose and usage rights": "هدف و حقوق استفاده",
  "Your API credentials and configuration are stored on this computer. Saved provider keys are protected with Windows account encryption. Usage history and diagnostic logs also remain local. When you send a request, the request and provider key are sent to the provider you configured so it can generate a response; they are not sent to the app developer. The local gateway token is written to Claude settings only when you choose to apply them. Prompts, completions, and provider response bodies are not written to diagnostic logs.": "نشانی ارائه‌دهنده، کلید API و تنظیمات شما روی همین رایانه ذخیره می‌شوند. کلیدهای ذخیره‌شده با رمزنگاری حساب ویندوز محافظت می‌شوند و تاریخچهٔ مصرف و گزارش خطاها نیز محلی می‌مانند. هنگام ارسال درخواست، متن درخواست و کلید ارائه‌دهنده برای پردازش به همان ارائه‌دهنده‌ای فرستاده می‌شوند که انتخاب کرده‌اید؛ این اطلاعات برای توسعه‌دهندهٔ برنامه ارسال نمی‌شوند. توکن درگاه محلی فقط وقتی در تنظیمات کلاد نوشته می‌شود که خودتان اعمال تنظیمات را انتخاب کنید. متن گفتگو و پاسخ ارائه‌دهنده در گزارش خطاها ذخیره نمی‌شود.",
  "This software was created for Shia researchers, scholars, and developers seeking to advance the interests of Islam and the Islamic Revolution. It is free to use for personal, academic, educational, research, and other non-commercial purposes. Commercial use, paid distribution, or inclusion in a revenue-generating service requires prior written permission from the developer. Third-party components remain subject to their own licenses.": "این نرم‌افزار برای پژوهشگران، محققان و توسعه‌دهندگان شیعه با هدف کمک به پیشبرد منافع اسلام و انقلاب اسلامی تهیه شده است. استفادهٔ شخصی، دانشگاهی، آموزشی، پژوهشی و دیگر استفاده‌های غیرتجاری از آن رایگان است. استفادهٔ تجاری، توزیع پولی یا به‌کارگیری در خدمات درآمدزا نیازمند دریافت اجازهٔ کتبی قبلی از توسعه‌دهنده است. مجوز اجزای متعلق به اشخاص ثالث تابع شرایط خودشان است.",
  "This is a custom non-commercial license, not an OSI-approved open-source license. Copyright and all rights not expressly granted remain with the developer.": "این یک مجوز اختصاصیِ استفادهٔ غیرتجاری است و مجوز متن‌بازِ تأییدشده از سوی OSI نیست. حق نشر و همهٔ حقوقی که صریحاً واگذار نشده‌اند برای توسعه‌دهنده محفوظ‌اند.",
  "Full terms are included in the LICENSE file distributed with the source code.": "متن کامل شرایط در فایل LICENSE همراه با کد منبع ارائه شده است.",
  "Got it": "متوجه شدم",
  "Close Claude Proxy?": "کلاد پروکسی بسته شود؟",
  "Would you like to keep Claude Proxy running in the system tray, or quit the app completely?": "مایلید کلاد پروکسی در سینی سیستم (ناحیهٔ اعلان‌ها) به اجرا ادامه دهد یا برنامه را کاملاً ببندید؟",
  "Cancel": "انصراف",
  "Keep running in the system tray": "ادامهٔ اجرا در سینی سیستم",
  "Quit completely": "بستن کامل برنامه",
  "Close dialog": "بستن پنجرهٔ گفتگو",
  "LOCAL TROUBLESHOOTING": "عیب‌یابی محلی",
  "Review proxy and provider issues, then copy or download a small report to share with the developer.": "مشکلات پروکسی و ارائه‌دهنده را بررسی کنید؛ سپس گزارشی کوتاه برای فرستادن به توسعه‌دهنده کپی یا بارگیری کنید.",
  "Filter log entries": "فیلتر گزارش‌ها",
  "All": "همه",
  "Problems": "مشکلات",
  "↻ Refresh": "↻ تازه‌سازی",
  "Copy report": "کپی گزارش",
  "Download report": "بارگیری گزارش",
  "Clear logs": "پاک‌کردن لاگ‌ها",
  "LOCAL DIAGNOSTICS": "گزارش‌های محلی",
  "Delete diagnostic logs?": "گزارش‌های خطا پاک شوند؟",
  "This removes the diagnostic logs stored on this computer. You cannot undo this action.": "گزارش‌های عیب‌یابی ذخیره‌شده روی این رایانه حذف می‌شوند و این کار قابل بازگشت نیست.",
  "Your token usage history will remain untouched.": "تاریخچهٔ مصرف توکن پاک نمی‌شود.",
  "Deleting…": "در حال پاک‌کردن…",
  "Clear history": "پاک‌کردن تاریخچه",
  "Open the Windows app to view logs": "برای دیدن گزارش‌ها، برنامهٔ ویندوز را باز کنید",
  "The browser preview cannot read or change the local diagnostic history.": "پیش‌نمایش مرورگر به تاریخچهٔ خطاهای محلی دسترسی ندارد.",
  "Recent diagnostic entries": "گزارش‌های اخیر",
  "No diagnostics yet": "هنوز گزارشی ثبت نشده است",
  "Proxy starts, configuration changes, model discovery problems, provider errors, and token-history issues will appear here.": "اجرای پروکسی، تغییر تنظیمات، مشکل در یافتن مدل‌ها، خطاهای ارائه‌دهنده و ایرادهای تاریخچهٔ توکن در اینجا نمایش داده می‌شوند.",
  "Reports stay on this computer until you copy or download them. Token history and diagnostics exclude conversation content and credentials.": "گزارش‌ها تا زمانی که خودتان آن‌ها را کپی یا بارگیری نکنید، روی همین رایانه می‌مانند. تاریخچهٔ توکن و گزارش‌های خطا شامل متن گفتگو یا اطلاعات ورود نیستند.",
  "Clear the diagnostic history saved on this computer?": "تاریخچهٔ خطاهای ذخیره‌شده روی این رایانه پاک شود؟",
  "Clipboard access is unavailable. Download the report instead.": "دسترسی به حافظهٔ موقت ممکن نیست؛ گزارش را بارگیری کنید.",
  "Diagnostic history cleared.": "تاریخچهٔ خطاها پاک شد.",
  "Diagnostic report copied.": "گزارش خطا کپی شد.",
  "Diagnostic report downloaded.": "گزارش خطا بارگیری شد.",
  "Loading local diagnostics…": "در حال بارگذاری گزارش‌های محلی…",
  "Log data is stored locally. Prompts, completions, provider response bodies, and API keys are excluded.": "داده‌های گزارش به‌صورت محلی ذخیره می‌شوند. متن درخواست‌ها، پاسخ‌ها، بدنهٔ پاسخ ارائه‌دهنده و کلیدهای API در آن‌ها ثبت نمی‌شوند.",
  "Diagnostic log controls": "ابزارهای گزارش خطا",
  "USAGE ANALYTICS": "تحلیل مصرف",
  "Track provider-reported token use over time, with clear labels when the provider does not return exact counts.": "مصرف توکن اعلام‌شده از سوی ارائه‌دهنده را در طول زمان ببینید؛ اگر تعداد دقیق در دسترس نباشد، برآوردها مشخص می‌شوند.",
  "Usage report controls": "تنظیمات گزارش مصرف",
  "Date range preset": "بازهٔ زمانی",
  "Date range": "بازهٔ زمانی",
  "Past hour": "یک ساعت گذشته",
  "Past day": "یک روز گذشته",
  "7 days": "۷ روز",
  "30 days": "۳۰ روز",
  "Custom": "دلخواه",
  "From": "از تاریخ",
  "Through": "تا تاریخ",
  "Group by date": "گروه‌بندی زمانی",
  "Daily": "روزانه",
  "Monthly": "ماهانه",
  "Usage breakdown": "نمایش مصرف",
  "Total": "کل مصرف",
  "By model": "بر اساس مدل",
  "Model": "مدل",
  "All providers": "همهٔ ارائه‌دهندگان",
  "All models": "همهٔ مدل‌ها",
  "Unknown provider": "ارائه‌دهندهٔ نامشخص",
  "Provider and model filters": "فیلتر ارائه‌دهنده و مدل",
  "Refresh every": "بازهٔ به‌روزرسانی",
  "Off": "خاموش",
  "5 seconds": "۵ ثانیه",
  "15 seconds": "۱۵ ثانیه",
  "30 seconds": "۳۰ ثانیه",
  "1 minute": "۱ دقیقه",
  "5 minutes": "۵ دقیقه",
  "Refresh token usage": "به‌روزرسانی مصرف توکن",
  "Update now": "به‌روزرسانی اکنون",
  "↻ Update now": "↻ به‌روزرسانی اکنون",
  "Usage totals": "جمع مصرف",
  "Input tokens": "توکن‌های ورودی",
  "Output tokens": "توکن‌های خروجی",
  "Cache tokens": "توکن‌های کش",
  "Reported cache tokens are split out; unreported input stays here.": "توکن‌های کشِ گزارش‌شده جدا نمایش داده می‌شوند؛ ورودیِ بدون جزئیات کش همان‌طور که ارائه‌دهنده اعلام کرده باقی می‌ماند.",
  "Provider input; cache usage is not reported.": "ورودی مطابق گزارش ارائه‌دهنده است؛ آمار کش گزارش نشده است.",
  "Generated response tokens": "توکن‌های تولیدشده در پاسخ",
  "Not reported by provider": "از سوی ارائه‌دهنده گزارش نشده",
  "Hit rate": "نرخ استفاده از کش",
  "Input excludes cache tokens when the provider reports them; unknown cache stays in input. Cache combines reads and creation. Output is generated separately.": "ورودی فقط به‌اندازهٔ توکن‌های کشی که ارائه‌دهنده گزارش کرده کاهش می‌یابد؛ کشِ گزارش‌نشده در ورودی باقی می‌ماند. مقدار کش، خواندن و ایجاد کش را با هم جمع می‌کند. خروجی جداگانه تولید می‌شود.",
  "Cache read": "خواندن از کش",
  "Cache creation": "ایجاد کش",
  "Input and output by provider model; cache read and creation are separate when reported.": "ورودی و خروجی بر اساس مدل ارائه‌دهنده؛ خواندن و ایجاد کش نیز در صورت گزارش جدا شده‌اند.",
  "Prompt cache breakdown": "تفکیک کش ورودی",
  "Provider-reported cache usage for the selected date range.": "مصرف کشی که ارائه‌دهنده در بازهٔ انتخاب‌شده گزارش کرده است.",
  "Cache stats not reported": "آمار کش گزارش نشده",
  "Provider models, cache details when reported, and Claude routes used in this date range.": "مدل‌های ارائه‌دهنده، جزئیات کش در صورت گزارش، و مسیرهای کلاد استفاده‌شده در این بازه.",
  "Read from cache": "خوانده‌شده از کش",
  "Newly processed input": "ورودیِ تازه‌پردازش‌شده",
  "Written to cache": "ذخیره‌شده در کش",
  "Generated output": "خروجیِ تولیدشده",
  "Provider did not include exact input totals": "ارائه‌دهنده مجموع دقیق ورودی را اعلام نکرده است",
  "Cache writes are included in this amount.": "توکن‌های نوشته‌شده در کش نیز در این مقدار هستند.",
  "Newly processed input saved for future reuse.": "ورودی تازه‌پردازش‌شده که برای استفادهٔ بعدی در کش ذخیره شده است.",
  "Output tokens are generated; prompt cache is for input.": "توکن‌های خروجی تولید می‌شوند؛ کش مربوط به ورودی است.",
  "Cache reads are reused input. Cache writes are newly processed input and are included in the new-input amount. Cache columns may cover only the requests listed above.": "خواندن از کش یعنی استفادهٔ دوباره از ورودی. نوشتن در کش، ورودی تازه‌پردازش‌شده است و در مقدار ورودی تازه هم محاسبه می‌شود. آمار کش ممکن است فقط بخشی از درخواست‌ها را پوشش دهد.",
  "The provider did not report cache details for this range.": "ارائه‌دهنده در این بازه جزئیات کش را گزارش نکرده است.",
  "Cached-token counts and cache percentages cannot be inferred when the provider omits cache usage fields.": "اگر ارائه‌دهنده فیلدهای مصرف کش را نفرستد، تعداد و درصد توکن‌های کش‌شده قابل محاسبه نیست.",
  "Input total": "کل ورودی",
  "New input": "ورودی تازه",
  "Combined tokens": "مجموع توکن‌ها",
  "Models used": "مدل‌های استفاده‌شده",
  "Provider usage": "آمار ارائه‌دهنده",
  "Daily usage": "مصرف روزانه",
  "Monthly usage": "مصرف ماهانه",
  "Input and output token totals": "مجموع توکن‌های ورودی و خروجی",
  "Input, cache, and output token totals": "مجموع توکن‌های ورودی، کش و خروجی",
  "Input and output tokens": "توکن‌های ورودی و خروجی",
  "Input, cache, and output tokens": "توکن‌های ورودی، کش و خروجی",
  "Tokens by provider model": "توکن‌ها بر اساس مدل ارائه‌دهنده",
  "Other models": "مدل‌های دیگر",
  "· Input:": "· ورودی:",
  "· Output:": "· خروجی:",
  "info": "اطلاع",
  "warning": "هشدار",
  "error": "خطا",
  "Token totals for each provider model": "مجموع توکن‌ها برای هر مدل ارائه‌دهنده",
  "Chart legend": "راهنمای نمودار",
  "Hover or focus chart marks for details; the table labels totals that include estimates.": "برای دیدن جزئیات، نشانگر را روی نمودار ببرید یا با صفحه‌کلید روی آن تمرکز کنید. جدول مشخص می‌کند کدام اعداد برآوردی‌اند.",
  "Token counts were supplied by the provider. ": "تعداد توکن‌ها از سوی ارائه‌دهنده اعلام شده است. ",
  "No usage in this date range": "در این بازه مصرفی ثبت نشده است",
  "Usage is available in the Windows app": "آمار مصرف در برنامهٔ ویندوز در دسترس است",
  "Successful requests through the local proxy will appear here. Usage history is stored on this computer.": "درخواست‌های موفق از طریق پروکسی محلی در اینجا نمایش داده می‌شوند. تاریخچهٔ مصرف روی همین رایانه ذخیره می‌شود.",
  "The browser preview does not have access to your local proxy history. Open the installed desktop app to view real usage.": "پیش‌نمایش مرورگر به تاریخچهٔ پروکسی محلی دسترسی ندارد. برای دیدن آمار واقعی، برنامهٔ دسکتاپ را باز کنید.",
  "Model breakdown": "تفکیک بر اساس مدل",
  "Provider models and Claude routes used in this date range.": "مدل‌های ارائه‌دهنده و مسیرهای کلاد استفاده‌شده در این بازه.",
  "Claude route": "مسیر کلاد",
  "Requests": "درخواست‌ها",
  "Input": "ورودی",
  "Output": "خروجی",
  "Source": "منبع",
  "No successful requests recorded for this range.": "در این بازه درخواست موفقی ثبت نشده است.",
  "Try a wider date range or reset the provider and model filters.": "بازهٔ زمانی گسترده‌تری انتخاب کنید یا فیلتر ارائه‌دهنده و مدل را بازنشانی کنید.",
  "Show the last 30 days": "نمایش ۳۰ روز گذشته",
  "The local history stores timestamps, provider names, model IDs, token totals, and estimate flags. It never stores prompts, completions, API keys, or provider response bodies.": "تاریخچهٔ محلی زمان، نام ارائه‌دهنده، شناسهٔ مدل، تعداد توکن و وضعیت برآورد را نگه می‌دارد. متن درخواست‌ها، پاسخ‌ها، کلیدهای API یا بدنهٔ پاسخ ارائه‌دهنده را ذخیره نمی‌کند.",
  "The local history stores timestamps, provider names, model IDs, token totals, provider-reported cache counts, and estimate flags. It never stores prompts, completions, API keys, or provider response bodies.": "تاریخچهٔ محلی زمان، نام ارائه‌دهنده، شناسهٔ مدل، تعداد توکن، آمار کشِ گزارش‌شده از سوی ارائه‌دهنده و وضعیت برآورد را نگه می‌دارد. متن درخواست‌ها، پاسخ‌ها، کلیدهای API یا بدنهٔ پاسخ ارائه‌دهنده را ذخیره نمی‌کند.",
  "The local history stores timestamps, model IDs, token totals, and estimate flags. It never stores prompts, completions, API keys, or provider response bodies.": "تاریخچهٔ محلی زمان، شناسهٔ مدل، تعداد توکن و وضعیت برآورد را نگه می‌دارد. متن درخواست‌ها، پاسخ‌ها، کلیدهای API یا بدنهٔ پاسخ ارائه‌دهنده را ذخیره نمی‌کند.",
  "Includes estimates": "شامل برآورد",
  "Provider reported": "اعلام‌شده از سوی ارائه‌دهنده",
  "Waiting for usage": "در انتظار آمار مصرف",
  "Updating": "در حال به‌روزرسانی",
  "Updating…": "در حال به‌روزرسانی…",
  "Choose a valid date range.": "یک بازهٔ زمانی معتبر انتخاب کنید.",
  "The operation could not be completed. Check your settings and connection.": "انجام عملیات ممکن نشد. تنظیمات و اتصال را بررسی کنید.",
  "Add a provider URL, API key, and at least one model route before starting the proxy.": "برای اجرای پروکسی، نشانی و کلید API ارائه‌دهنده و دست‌کم یک مسیر مدل اضافه کنید.",
  "Local proxy started.": "پروکسی محلی اجرا شد.",
  "Proxy stopped.": "پروکسی متوقف شد.",
  "Settings saved.": "تنظیمات ذخیره شد.",
  "Address copied.": "نشانی کپی شد.",
  "Clipboard access is unavailable.": "دسترسی به حافظهٔ موقت ممکن نیست.",
  "The local proxy is ready.": "پروکسی محلی آماده است.",
  "Claude Desktop settings were updated. Fully quit and reopen the app.": "تنظیمات کلاد دسکتاپ به‌روز شد. برنامه را کاملاً ببندید و دوباره باز کنید.",
  "Claude Code settings were updated. Close and reopen any running CLI sessions.": "تنظیمات کلاد کد به‌روز شد. نشست‌های فعال خط فرمان را ببندید و دوباره باز کنید.",
  "Enter a provider URL before checking its API.": "پیش از بررسی API، نشانی ارائه‌دهنده را وارد کنید.",
  "The proxy is unavailable in the browser preview.": "پروکسی در پیش‌نمایش مرورگر در دسترس نیست.",
  "Model discovery is available in the desktop app.": "یافتن مدل‌ها در برنامهٔ دسکتاپ در دسترس است.",
  "Provider API checks are available in the desktop app.": "بررسی API ارائه‌دهنده در برنامهٔ دسکتاپ در دسترس است.",
  "The local route check is available in the desktop app.": "بررسی مسیر محلی در برنامهٔ دسکتاپ در دسترس است.",
  "Claude Desktop setup is available in the desktop app.": "راه‌اندازی کلاد دسکتاپ در برنامهٔ دسکتاپ در دسترس است.",
  "Claude Code setup is available in the desktop app.": "راه‌اندازی کلاد کد در برنامهٔ دسکتاپ در دسترس است.",
  "The provider URL must start with http:// or https://.": "نشانی ارائه‌دهنده باید با ‎http:// یا ‎https:// آغاز شود.",
  "The provider returned no models.": "ارائه‌دهنده هیچ مدلی برنگرداند.",
  "The provider response did not contain a data or models array.": "پاسخ ارائه‌دهنده شامل آرایهٔ data یا models نبود.",
  "The provider's /models response was not valid JSON.": "پاسخ ‎/models ارائه‌دهنده قالب JSON معتبری نداشت.",
  "This Claude role already has a route.": "برای این نقش کلاد از قبل یک مسیر تعریف شده است.",
  "Choose a Claude role and enter its upstream model ID.": "یک نقش کلاد انتخاب و شناسهٔ مدل بالادستی آن را وارد کنید.",
  "No provider URL added": "نشانی ارائه‌دهنده وارد نشده است",
  "Not selected yet": "هنوز انتخاب نشده است",
  "Save settings": "ذخیرهٔ تنظیمات",
  "Save model routes": "ذخیرهٔ مسیرهای مدل",
  "Check connection and fetch models": "بررسی اتصال و دریافت مدل‌ها",
  "Fetch model list": "دریافت فهرست مدل‌ها",
  "Fetch models": "دریافت مدل‌ها",
  "Ping provider API": "آزمون پینگ API ارائه‌دهنده",
  "Start proxy": "اجرای پروکسی",
  "Stop proxy": "توقف پروکسی",
  "Start local proxy": "اجرای پروکسی محلی",
  "Stop local proxy": "توقف پروکسی محلی",
  "Proxy running": "پروکسی در حال اجراست",
  "Proxy stopped": "پروکسی متوقف است",
  "Updating proxy…": "در حال به‌روزرسانی پروکسی…",
  "Pinging provider…": "در حال پینگ ارائه‌دهنده…",
  "Pinging…": "در حال پینگ…",
  "Checking…": "در حال بررسی…",
  "Fetching…": "در حال دریافت…",
  "Saving…": "در حال ذخیره…",
  "Continue": "ادامه",
  "Finish": "پایان",
  "Save and finish": "ذخیره و پایان",
  "Model discovery": "یافتن مدل‌ها",
  "No provider or key is preconfigured.": "هیچ ارائه‌دهنده یا کلیدی از پیش تنظیم نشده است.",
};

export function translateText(source: string, language: Language): string {
  if (language === "en") return source;
  const trimmed = source.trim();
  const normalized = trimmed.replace(/\s+/g, " ");
  let result = fa[normalized];
  if (!result) {
    let match = normalized.match(/^([\d۰-۹٠-٩٬,]+) models retrieved from the provider\.$/);
    if (match) result = `${match[1]} مدل از ارائه‌دهنده دریافت شد.`;
    match ??= normalized.match(/^([\d۰-۹٠-٩٬,]+) model retrieved from the provider\.$/);
    if (!result && match) result = `${match[1]} مدل از ارائه‌دهنده دریافت شد.`;
    match ??= normalized.match(/^([\d۰-۹٠-٩٬,]+) models retrieved$/);
    if (!result && match) result = `${match[1]} مدل دریافت شد`;
    match = normalized.match(/^([\d۰-۹٠-٩٬,]+) routes$/);
    if (!result && match) result = `${match[1]} مسیر`;
    match = normalized.match(/^([\d۰-۹٠-٩٬,]+) successful requests$/);
    if (!result && match) result = `${match[1]} درخواست موفق`;
    match = normalized.match(/^([\d۰-۹٠-٩٬,]+) request estimates$/);
    if (!result && match) result = `${match[1]} برآورد درخواست`;
    match = normalized.match(/^(.+) \/ (.+) requests reported cache details$/);
    if (!result && match) result = `${match[1]} از ${match[2]} درخواست دارای آمار کش`;
    match = normalized.match(/^(.+) \/ (.+) requests reported cache usage$/);
    if (!result && match) result = `آمار کش در ${match[1]} از ${match[2]} درخواست گزارش شده`;
    match = normalized.match(/^(.+)% of (.+) cache-reported input$/);
    if (!result && match) result = `${match[1]}٪ از ${match[2]} ورودیِ دارای آمار کش`;
    match = normalized.match(/^Cache read share: (.+)%$/);
    if (!result && match) result = `سهم ورودیِ خوانده‌شده از کش: ${match[1]}٪`;
    match = normalized.match(/^([\d۰-۹٠-٩٬,]+) requests$/);
    if (!result && match) result = `${match[1]} درخواست`;
    match = normalized.match(/^Updated (.+)$/);
    if (!result && match) result = `آخرین به‌روزرسانی: ${match[1]}`;
    match = normalized.match(/^Step (\d+) of (\d+)$/);
    if (!result && match) result = `گام ${match[1]} از ${match[2]}`;
    match = normalized.match(/^(\d+) entries · (\d+) warnings and errors$/);
    if (!result && match) result = `${match[1]} مورد · ${match[2]} هشدار و خطا`;
    match = normalized.match(/^Estimated usage: input for (.+) requests and output for (.+) requests\.$/);
    if (!result && match) result = `مصرف برآوردی: ورودی برای ${match[1]} درخواست و خروجی برای ${match[2]} درخواست.`;
    match = normalized.match(/^(.+): (.+) input tokens$/);
    if (!result && match) result = `${match[1]}: ${match[2]} توکن ورودی`;
    match = normalized.match(/^(.+): (.+) cache read tokens$/);
    if (!result && match) result = `${match[1]}: ${match[2]} توکن خوانده‌شده از کش`;
    match = normalized.match(/^(.+): (.+) cache creation tokens$/);
    if (!result && match) result = `${match[1]}: ${match[2]} توکن ایجاد کش`;
    match = normalized.match(/^(.+): (.+) output tokens$/);
    if (!result && match) result = `${match[1]}: ${match[2]} توکن خروجی`;
    match = normalized.match(/^(.+) · Input: (.+) tokens$/);
    if (!result && match) result = `${match[1]} · ورودی: ${match[2]} توکن`;
    match = normalized.match(/^(.+) · Cache read: (.+) tokens$/);
    if (!result && match) result = `${match[1]} · خوانده‌شده از کش: ${match[2]} توکن`;
    match = normalized.match(/^(.+) · Cache creation: (.+) tokens$/);
    if (!result && match) result = `${match[1]} · ایجاد کش: ${match[2]} توکن`;
    match = normalized.match(/^(.+) · Output: (.+) tokens$/);
    if (!result && match) result = `${match[1]} · خروجی: ${match[2]} توکن`;
    match = normalized.match(/^(.+) · (.+): (.+) tokens$/);
    if (!result && match) result = `${match[1]} · ${match[2]}: ${match[3]} توکن`;
    match = normalized.match(/^(.+), (.+), (.+) tokens$/);
    if (!result && match) result = `${match[1]}، ${match[2]}، ${match[3]} توکن`;
    match = normalized.match(/^(.+) over (\d+) periods?$/);
    if (!result && match) result = `${match[1]} در ${match[2]} بازه`;
    match = normalized.match(/^Provider API responded successfully in ([\d.]+) ms\.$/);
    if (!result && match) result = `API ارائه‌دهنده در ${match[1]} میلی‌ثانیه با موفقیت پاسخ داد.`;
    match = normalized.match(/^Provider API responded in ([\d.]+) ms \(HTTP (\d+)\)\.$/);
    if (!result && match) result = `API ارائه‌دهنده در ${match[1]} میلی‌ثانیه پاسخ داد (HTTP ${match[2]}).`;
    match = normalized.match(/^Provider API responded with HTTP (\d+) in ([\d.]+) ms\.$/);
    if (!result && match) result = `API ارائه‌دهنده با HTTP ${match[1]} و در ${match[2]} میلی‌ثانیه پاسخ داد.`;
    match = normalized.match(/^The provider returned HTTP (401|403) for (.+)\. Check the API key and authentication style\.$/);
    if (!result && match) result = `ارائه‌دهنده برای ${match[2]} با HTTP ${match[1]} پاسخ داد. کلید API و روش احراز هویت را بررسی کنید.`;
    match = normalized.match(/^The provider returned HTTP 404 for (.+)\. Check that the base URL includes the provider's compatible API path\.$/);
    if (!result && match) result = `ارائه‌دهنده برای ${match[1]} با HTTP 404 پاسخ داد. بررسی کنید نشانی پایه مسیر API سازگار ارائه‌دهنده را داشته باشد.`;
    match = normalized.match(/^The provider gateway returned HTTP (502|503|504) \(Bad Gateway\)\. The server was reached, but its upstream service is unavailable\. Verify the API URL and provider availability, then try again\.$/);
    if (!result && match) result = `درگاه ارائه‌دهنده با HTTP ${match[1]} پاسخ داد و سرویس بالادستی در دسترس نیست. نشانی API و وضعیت ارائه‌دهنده را بررسی و دوباره تلاش کنید.`;
    match = normalized.match(/^Could not reach the provider API: (.+)$/);
    if (!result && match) result = `ارتباط با API ارائه‌دهنده برقرار نشد: ${match[1]}`;
    match = normalized.match(/^Web access is working through manual proxy (.+)\.$/);
    if (!result && match) result = `دسترسی وب از طریق پروکسی دستی ${match[1]} فعال است.`;
    match = normalized.match(/^The provider URL is invalid: (.+)$/);
    if (!result && match) result = `نشانی ارائه‌دهنده معتبر نیست: ${match[1]}`;
  }
  // Keep internal whitespace (especially line breaks in the license and log text)
  // when a node has no dictionary entry. The normalized value is only for lookup.
  if (!result) result = trimmed;
  result = result.replace(/[۰-۹٠-٩]/g, (digit) => latinDigits[digit] ?? digit)
    .replace(/٬/g, ",")
    .replace(/٫/g, ".");
  const leading = source.match(/^\s*/)?.[0] ?? "";
  const trailing = source.match(/\s*$/)?.[0] ?? "";
  return `${leading}${result}${trailing}`;
}

type AttributeSources = Map<string, string>;
const textSources = new WeakMap<Text, string>();
const lastTextValues = new WeakMap<Text, string>();
const attributeSources = new WeakMap<Element, AttributeSources>();
const lastAttributeValues = new WeakMap<Element, AttributeSources>();
const translatedAttributes = ["aria-label", "aria-description", "title", "placeholder", "alt"];

function translateTextNode(node: Text, language: Language) {
  const current = node.data;
  const last = lastTextValues.get(node);
  const source = textSources.get(node);
  if (source === undefined || current !== last) textSources.set(node, current);
  const original = textSources.get(node) ?? current;
  const translated = translateText(original, language);
  if (current !== translated) node.data = translated;
  lastTextValues.set(node, translated);
}

function translateAttributes(element: Element, language: Language) {
  let sources = attributeSources.get(element);
  if (!sources) { sources = new Map(); attributeSources.set(element, sources); }
  let lastValues = lastAttributeValues.get(element);
  if (!lastValues) { lastValues = new Map(); lastAttributeValues.set(element, lastValues); }
  for (const name of translatedAttributes) {
    const current = element.getAttribute(name);
    if (current === null) continue;
    if (!sources.has(name) || current !== lastValues.get(name)) sources.set(name, current);
    const original = sources.get(name) ?? current;
    const translated = translateText(original, language);
    if (current !== translated) element.setAttribute(name, translated);
    lastValues.set(name, translated);
  }
}

function localizeNode(node: Node, language: Language) {
  if (node.nodeType === Node.TEXT_NODE) {
    const parent = node.parentElement;
    if (parent?.closest("code, pre, [translate='no']")) return;
    translateTextNode(node as Text, language);
    return;
  }
  if (node.nodeType !== Node.ELEMENT_NODE) return;
  const element = node as Element;
  translateAttributes(element, language);
  for (const child of element.childNodes) localizeNode(child, language);
}

/** Keeps UI copy in React's rendered tree synchronized with the chosen language. */
export function observeLocalizedContent(root: Element, language: Language): () => void {
  localizeNode(root, language);
  const observer = new MutationObserver((records) => {
    for (const record of records) {
      if (record.type === "characterData" && record.target.nodeType === Node.TEXT_NODE) {
        translateTextNode(record.target as Text, language);
      } else if (record.type === "attributes" && record.target instanceof Element) {
        translateAttributes(record.target, language);
      } else {
        record.addedNodes.forEach((node) => localizeNode(node, language));
      }
    }
  });
  observer.observe(root, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: translatedAttributes });
  return () => observer.disconnect();
}

export function localeFor(language: Language): string {
  return language === "fa" ? "fa-IR-u-nu-latn" : "en-US";
}
