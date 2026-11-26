// The thank-you page shown after payment, and the key email. Small, 10 languages, no tracking.
import type { Mailer } from "./service.ts";

type Words = { title: string; key: string; how: string; copy: string; copied: string; keep: string; error: string; mailSubject: string; mailIntro: string };

const W: Record<string, Words> = {
  en: { title: "Thank you!", key: "Your Pro key", how: "In the app: Settings → Licence → paste the key → Activate. It works on up to 3 computers.", copy: "Copy", copied: "Copied", keep: "Keep this key somewhere safe. You can get it again by email from the app.", error: "We could not find this payment. If you were charged, write to us and we will sort it out.", mailSubject: "Your Pro key", mailIntro: "Thank you for your purchase. Here is your key:" },
  fr: { title: "Merci !", key: "Ta clé Pro", how: "Dans l'app : Réglages → Licence → colle la clé → Activer. Elle fonctionne sur 3 ordinateurs maximum.", copy: "Copier", copied: "Copiée", keep: "Garde cette clé en lieu sûr. Tu peux la recevoir à nouveau par e-mail depuis l'app.", error: "Paiement introuvable. Si tu as été débité, écris-nous et on règle ça.", mailSubject: "Ta clé Pro", mailIntro: "Merci pour ton achat. Voici ta clé :" },
  es: { title: "¡Gracias!", key: "Tu clave Pro", how: "En la app: Ajustes → Licencia → pega la clave → Activar. Funciona en hasta 3 ordenadores.", copy: "Copiar", copied: "Copiada", keep: "Guarda esta clave en un lugar seguro. Puedes recibirla de nuevo por correo desde la app.", error: "No encontramos este pago. Si se te cobró, escríbenos y lo solucionamos.", mailSubject: "Tu clave Pro", mailIntro: "Gracias por tu compra. Esta es tu clave:" },
  pt: { title: "Obrigado!", key: "Sua chave Pro", how: "No app: Configurações → Licença → cole a chave → Ativar. Funciona em até 3 computadores.", copy: "Copiar", copied: "Copiada", keep: "Guarde esta chave em um lugar seguro. Você pode recebê-la de novo por e-mail pelo app.", error: "Não encontramos este pagamento. Se você foi cobrado, escreva para nós e resolvemos.", mailSubject: "Sua chave Pro", mailIntro: "Obrigado pela compra. Esta é a sua chave:" },
  de: { title: "Danke!", key: "Dein Pro-Schlüssel", how: "In der App: Einstellungen → Lizenz → Schlüssel einfügen → Aktivieren. Er funktioniert auf bis zu 3 Computern.", copy: "Kopieren", copied: "Kopiert", keep: "Bewahre diesen Schlüssel sicher auf. Du kannst ihn in der App erneut per E-Mail anfordern.", error: "Diese Zahlung wurde nicht gefunden. Falls du belastet wurdest, schreib uns – wir klären das.", mailSubject: "Dein Pro-Schlüssel", mailIntro: "Danke für deinen Kauf. Hier ist dein Schlüssel:" },
  ja: { title: "ありがとうございます！", key: "Pro キー", how: "アプリで：設定 → ライセンス → キーを貼り付け → 有効化。最大 3 台のパソコンで使えます。", copy: "コピー", copied: "コピーしました", keep: "このキーは大切に保管してください。アプリからメールで再送できます。", error: "この支払いが見つかりません。請求された場合はご連絡ください。対応します。", mailSubject: "Pro キー", mailIntro: "ご購入ありがとうございます。キーはこちらです：" },
  ko: { title: "감사합니다!", key: "Pro 키", how: "앱에서: 설정 → 라이선스 → 키 붙여넣기 → 활성화. 최대 3대의 컴퓨터에서 사용할 수 있습니다.", copy: "복사", copied: "복사됨", keep: "이 키를 안전한 곳에 보관하세요. 앱에서 이메일로 다시 받을 수 있습니다.", error: "이 결제를 찾을 수 없습니다. 청구되었다면 연락해 주세요. 해결해 드리겠습니다.", mailSubject: "Pro 키", mailIntro: "구매해 주셔서 감사합니다. 키는 다음과 같습니다:" },
  hi: { title: "धन्यवाद!", key: "आपकी Pro कुंजी", how: "ऐप में: सेटिंग्स → लाइसेंस → कुंजी चिपकाएँ → सक्रिय करें। यह 3 कंप्यूटरों तक चलती है।", copy: "कॉपी करें", copied: "कॉपी हो गई", keep: "यह कुंजी सुरक्षित जगह रखें। ऐप से इसे दोबारा ईमेल पर मँगा सकते हैं।", error: "यह भुगतान नहीं मिला। अगर पैसे कटे हैं तो हमें लिखें, हम ठीक कर देंगे।", mailSubject: "आपकी Pro कुंजी", mailIntro: "ख़रीदारी के लिए धन्यवाद। आपकी कुंजी यह है:" },
  ar: { title: "شكرًا!", key: "مفتاح Pro الخاص بك", how: "في التطبيق: الإعدادات ← الترخيص ← الصق المفتاح ← تفعيل. يعمل على 3 حواسيب كحد أقصى.", copy: "نسخ", copied: "تم النسخ", keep: "احتفظ بهذا المفتاح في مكان آمن. يمكنك استلامه مجددًا بالبريد من التطبيق.", error: "لم نعثر على هذه الدفعة. إذا تم خصم المبلغ فراسلنا وسنحل الأمر.", mailSubject: "مفتاح Pro الخاص بك", mailIntro: "شكرًا لشرائك. هذا مفتاحك:" },
  id: { title: "Terima kasih!", key: "Kunci Pro Anda", how: "Di aplikasi: Pengaturan → Lisensi → tempel kunci → Aktifkan. Berlaku di hingga 3 komputer.", copy: "Salin", copied: "Tersalin", keep: "Simpan kunci ini di tempat aman. Anda bisa menerimanya lagi lewat email dari aplikasi.", error: "Pembayaran ini tidak ditemukan. Jika Anda sudah ditagih, hubungi kami dan kami akan membereskannya.", mailSubject: "Kunci Pro Anda", mailIntro: "Terima kasih atas pembelian Anda. Ini kunci Anda:" },
};

export const words = (lang: string): Words => W[lang] ?? W.en;

/** While key emails are not set up (they need a domain): no promise of an email. */
const KEEP_NO_MAIL: Record<string, string> = {
  en: "Keep this key somewhere safe. If you lose it, write to the support address on the website.",
  fr: "Garde cette clé en lieu sûr. En cas de perte, écris à l'adresse de support indiquée sur le site.",
  es: "Guarda esta clave en un lugar seguro. Si la pierdes, escribe a la dirección de soporte del sitio.",
  pt: "Guarde esta chave em um lugar seguro. Se perdê-la, escreva para o endereço de suporte do site.",
  de: "Bewahre diesen Schlüssel sicher auf. Falls du ihn verlierst, schreib an die Support-Adresse auf der Website.",
  ja: "このキーは大切に保管してください。なくした場合は、サイトに記載のサポート窓口までご連絡ください。",
  ko: "이 키를 안전한 곳에 보관하세요. 잃어버리면 웹사이트의 지원 주소로 연락해 주세요.",
  hi: "यह कुंजी सुरक्षित जगह रखें। खो जाए तो वेबसाइट पर दिए सहायता पते पर लिखें।",
  ar: "احتفظ بهذا المفتاح في مكان آمن. إذا فقدته، راسل عنوان الدعم المذكور في الموقع.",
  id: "Simpan kunci ini di tempat aman. Jika hilang, kirim pesan ke alamat dukungan di situs web.",
};

const esc = (s: string) => s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

export function successPage(lang: string, product: string, key: string | null, canEmail = false): string {
  const w = words(lang);
  const keep = canEmail ? w.keep : (KEEP_NO_MAIL[lang] ?? KEEP_NO_MAIL.en);
  const dir = lang === "ar" ? "rtl" : "ltr";
  const body = key
    ? `<h1>${esc(w.title)}</h1><p class="label">${esc(w.key)}</p>
       <div class="key"><code id="k">${esc(key)}</code><button id="c" type="button">${esc(w.copy)}</button></div>
       <p>${esc(w.how)}</p><p class="muted">${esc(keep)}</p>`
    : `<h1>${esc(product)}</h1><p>${esc(w.error)}</p>`;
  return `<!doctype html><html lang="${esc(lang)}" dir="${dir}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="robots" content="noindex"><title>${esc(product)}</title>
<style>:root{color-scheme:light dark;--bg:#fafafa;--fg:#18181b;--mu:#71717a;--bd:#e4e4e7;--card:#fff}
@media(prefers-color-scheme:dark){:root{--bg:#09090b;--fg:#ececee;--mu:#a1a1aa;--bd:#27272a;--card:#0f0f11}}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.55 system-ui,-apple-system,"Segoe UI",sans-serif;display:grid;place-items:center;min-height:100vh;padding:24px;box-sizing:border-box}
main{max-width:560px;width:100%;background:var(--card);border:1px solid var(--bd);border-radius:10px;padding:28px}
h1{margin:0 0 16px;font-size:26px}.label{margin:0 0 6px;color:var(--mu);font-size:13px;text-transform:uppercase;letter-spacing:.04em}
.key{display:flex;gap:8px;align-items:center;flex-wrap:wrap;margin-bottom:18px}code{font:600 19px ui-monospace,Consolas,monospace;padding:10px 12px;border:1px solid var(--bd);border-radius:6px;user-select:all}
button{font:inherit;padding:9px 14px;border-radius:6px;border:0;background:var(--fg);color:var(--bg);cursor:pointer}.muted{color:var(--mu);font-size:14px}</style></head>
<body><main>${body}</main>
${key ? `<script>document.getElementById("c").onclick=function(){navigator.clipboard.writeText(document.getElementById("k").textContent).then(()=>{this.textContent=${JSON.stringify(w.copied)}})}</script>` : ""}
</body></html>`;
}

/** Key emails through Resend (free plan: 3,000 emails a month). */
export function resendMailer(apiKey: string, from: string, product: string, fetcher: typeof fetch = fetch): Mailer {
  return {
    async sendKeys(to, keys, lang) {
      const w = words(lang.slice(0, 2));
      const text = `${w.mailIntro}\n\n${keys.join("\n")}\n\n${w.how}\n\n${w.keep}\n\n— ${product}`;
      const res = await fetcher("https://api.resend.com/emails", {
        method: "POST",
        headers: { Authorization: `Bearer ${apiKey}`, "Content-Type": "application/json" },
        body: JSON.stringify({ from, to: [to], subject: `${product} — ${w.mailSubject}`, text }),
      });
      if (!res.ok) throw new Error(`resend ${res.status}`);
    },
  };
}
