// Legal pages, French (reference) and English. Identity fields come from legal.local.json,
// which is NOT committed (the public repository stays pseudonymous); see legal.example.json.
// These are drafts to check, not legal advice: docs/phase-5-checklist.md lists what to verify.

export const LEGAL_PAGES = ["terms", "privacy", "licence"];

const fr = (v) => ({
  terms: {
    title: "Conditions de vente",
    body: `
<p>Dernière mise à jour : ${v.updated}.</p>
<h2>1. Objet</h2>
<p>Ces conditions encadrent l'achat d'une licence Pro du logiciel ${v.product}, édité par ${v.name}.</p>
<h2>2. Commande et paiement</h2>
<p>La commande se fait sur une page de paiement Stripe. Stripe agit comme revendeur et marchand officiel : il encaisse le paiement, applique et reverse la TVA ou les taxes de ton pays, et t'envoie le reçu et la facture. Les prix affichés sont toutes taxes comprises et adaptés au pays de l'acheteur.</p>
<h2>3. Offres</h2>
<ul>
<li><strong>Pro mensuel</strong> et <strong>Pro annuel</strong> : abonnement renouvelé automatiquement à chaque échéance, résiliable à tout moment depuis le lien du reçu. La résiliation prend effet à la fin de la période payée.</li>
<li><strong>Licence à vie</strong> : paiement unique, limitée aux 500 premiers acheteurs. Elle donne accès aux fonctions Pro et à leurs mises à jour tant que le logiciel est commercialisé. Si sa commercialisation s'arrête, chaque titulaire reçoit une licence définitive qui fonctionne hors ligne, sans limite de durée.</li>
</ul>
<h2>4. Essai</h2>
<p>Un essai Pro de 14 jours est offert, sans carte bancaire, une fois par ordinateur. Il s'arrête de lui-même, sans aucun prélèvement.</p>
<h2>5. Livraison</h2>
<p>La clé de licence s'affiche dès le paiement validé et t'est envoyée par e-mail. Elle s'active sur 3 ordinateurs au maximum ; tu peux libérer une place depuis l'app.</p>
<h2>6. Rétractation et remboursement</h2>
<p>La licence étant un contenu numérique fourni immédiatement, le droit légal de rétractation de 14 jours peut ne pas s'appliquer une fois son exécution commencée avec ton accord (article L221-28, 13° du Code de la consommation). Nous offrons néanmoins le <strong>remboursement intégral de tout paiement demandé dans les 14 jours</strong>, sans justification : écris à <a href="mailto:${v.email}">${v.email}</a>. Le remboursement met fin à la licence correspondante.</p>
<h2>7. Ce qui n'est pas inclus</h2>
<p>${v.product} pilote Claude Code, qui nécessite un abonnement Claude ou une clé API Anthropic, payés séparément auprès d'Anthropic. Les devis sont des estimations. Le plafond arrête une tâche dès qu'il est atteint ; un léger dépassement reste possible pendant l'arrêt.</p>
<h2>8. Responsabilité</h2>
<p>La vérification automatique réduit le risque d'erreur mais ne garantit pas qu'un code produit par une IA soit exempt de défauts : relis et teste avant toute mise en production. Les garanties légales de conformité des contenus numériques s'appliquent (articles L224-25-12 et suivants du Code de la consommation).</p>
<h2>9. Suspension</h2>
<p>Une licence peut être désactivée en cas de fraude, de revente ou de partage public de la clé, ou de contournement de la vérification de licence.</p>
<h2>10. Litiges</h2>
<p>En cas de difficulté, écris d'abord à <a href="mailto:${v.email}">${v.email}</a>. ${v.mediator ? `Tu peux aussi recourir gratuitement au médiateur de la consommation : ${v.mediator}.` : ""} Ces conditions sont soumises au droit français, sans te priver des règles protectrices de ton pays de résidence.</p>`,
  },
  privacy: {
    title: "Confidentialité",
    body: `
<p>Dernière mise à jour : ${v.updated}. Responsable du traitement : ${v.name}, <a href="mailto:${v.email}">${v.email}</a>.</p>
<h2>Ce que nous ne collectons jamais</h2>
<p>Ni ton code, ni tes demandes, ni tes fichiers : ${v.product} fonctionne sur ton ordinateur. Le site n'utilise ni cookie, ni outil de mesure d'audience, ni pistage.</p>
<h2>Licences et essais</h2>
<p>Pour gérer l'essai et les licences, l'app envoie au serveur de licences un <strong>code anonyme de l'ordinateur</strong> (une empreinte calculée à partir d'un identifiant système, qui ne permet pas de le retrouver), et ta clé quand tu l'actives. Base légale : l'exécution du contrat. Conservation : pendant la durée de la licence puis 3 ans ; 2 ans pour un essai.</p>
<h2>Achats</h2>
<p>Le paiement est traité par Stripe, qui agit comme marchand officiel et responsable de ses propres traitements (voir la politique de confidentialité de Stripe). Nous recevons de Stripe les identifiants de commande et d'abonnement, leur statut et ton adresse e-mail. L'adresse sert uniquement à t'envoyer ta clé ; nous n'en gardons qu'une empreinte, pour pouvoir te renvoyer tes clés si tu les perds. Base légale : l'exécution du contrat. Les données de commande sont conservées le temps exigé par la loi.</p>
<h2>Sous-traitants</h2>
<ul>
<li>Cloudflare (hébergement du site et du serveur de licences), qui conserve des journaux techniques (adresse IP) pour la sécurité.</li>
<li>Resend (envoi des e-mails contenant les clés).</li>
</ul>
<p>Ces prestataires sont situés aux États-Unis ; les transferts sont encadrés par le cadre de protection des données UE–États-Unis ou par les clauses contractuelles types de la Commission européenne.</p>
<h2>Claude Code et Anthropic</h2>
<p>L'app lance Claude Code avec ton propre compte Anthropic. Ce que Claude Code envoie à Anthropic relève de ta relation avec Anthropic ; nous n'y avons pas accès.</p>
<h2>Tes droits</h2>
<p>Tu peux demander l'accès, la rectification, l'effacement, la limitation ou la portabilité de tes données, ou t'opposer à leur traitement, en écrivant à <a href="mailto:${v.email}">${v.email}</a>. Tu peux aussi saisir la CNIL (cnil.fr).</p>`,
  },
  licence: {
    title: "Licence",
    body: `
<h2>Code source</h2>
<p>Le code de ${v.product} est public, sous <a href="${v.source}">Functional Source License (FSL-1.1-ALv2)</a> : tu peux le lire, l'utiliser, le modifier et le redistribuer, y compris pour un usage professionnel, sauf pour proposer un produit concurrent. Deux ans après sa publication, chaque version passe sous licence Apache-2.0.</p>
<h2>Licence Pro</h2>
<p>L'achat d'une licence Pro te donne un droit personnel, non exclusif et non transférable d'utiliser les fonctions Pro sur 3 ordinateurs au maximum, pendant la durée de ton abonnement ou à vie selon l'offre choisie. L'usage professionnel, y compris pour tes clients, est autorisé.</p>
<p>Il est interdit de revendre ou de partager publiquement ta clé, et de contourner la vérification de licence.</p>
<p>À la fin d'un abonnement ou après un remboursement, l'app revient à la version gratuite. Tes projets, ton historique et tes réglages restent sur ton ordinateur.</p>
<p>Le logiciel est fourni avec les garanties légales applicables aux contenus numériques, et sans autre garantie.</p>`,
  },
});

const en = (v) => ({
  terms: {
    title: "Terms of sale",
    body: `
<p>Last updated: ${v.updated}. The French version prevails.</p>
<h2>1. Purpose</h2>
<p>These terms cover the purchase of a Pro licence for ${v.product}, published by ${v.name}.</p>
<h2>2. Order and payment</h2>
<p>You order on a Stripe checkout page. Stripe acts as reseller and merchant of record: it collects the payment, applies and remits the VAT or taxes of your country, and sends you the receipt and invoice. Prices include taxes and are adapted to the buyer's country.</p>
<h2>3. Offers</h2>
<ul>
<li><strong>Pro monthly</strong> and <strong>Pro yearly</strong>: subscription renewed automatically at each term, cancellable at any time from the link in your receipt. Cancellation takes effect at the end of the paid period.</li>
<li><strong>Lifetime licence</strong>: one payment, limited to the first 500 buyers. It gives access to Pro features and their updates for as long as the software is sold. If sales stop, every holder receives a final licence that works offline with no time limit.</li>
</ul>
<h2>4. Trial</h2>
<p>A 14-day Pro trial is offered, without a card, once per computer. It ends by itself, with no charge.</p>
<h2>5. Delivery</h2>
<p>The licence key is shown as soon as the payment is confirmed and sent by email. It activates on up to 3 computers; you can free a seat from the app.</p>
<h2>6. Withdrawal and refunds</h2>
<p>As digital content supplied immediately, the legal 14-day right of withdrawal may not apply once performance has started with your consent. We nevertheless offer a <strong>full refund of any payment requested within 14 days</strong>, no questions asked: write to <a href="mailto:${v.email}">${v.email}</a>. A refund ends the corresponding licence.</p>
<h2>7. Not included</h2>
<p>${v.product} drives Claude Code, which requires a Claude subscription or an Anthropic API key, paid separately to Anthropic. Quotes are estimates. The cap stops a task as soon as it is reached; a small overrun remains possible while it stops.</p>
<h2>8. Liability</h2>
<p>Automatic verification lowers the risk of errors but cannot guarantee that AI-written code is free of defects: review and test before production use. Statutory conformity guarantees for digital content apply.</p>
<h2>9. Suspension</h2>
<p>A licence may be deactivated in case of fraud, resale or public sharing of the key, or circumvention of the licence check.</p>
<h2>10. Disputes</h2>
<p>If something goes wrong, write first to <a href="mailto:${v.email}">${v.email}</a>. ${v.mediator ? `You may also use the consumer mediator free of charge: ${v.mediator}.` : ""} These terms are governed by French law, without depriving you of the mandatory protections of your country of residence.</p>`,
  },
  privacy: {
    title: "Privacy",
    body: `
<p>Last updated: ${v.updated}. Controller: ${v.name}, <a href="mailto:${v.email}">${v.email}</a>. The French version prevails.</p>
<h2>What we never collect</h2>
<p>Not your code, your requests or your files: ${v.product} runs on your computer. The website uses no cookies, no analytics and no tracking.</p>
<h2>Licences and trials</h2>
<p>To manage trials and licences, the app sends the licence server an <strong>anonymous code of the computer</strong> (a fingerprint derived from a system identifier, which cannot be reversed) and your key when you activate it. Legal basis: performance of the contract. Retention: for the life of the licence, then 3 years; 2 years for a trial.</p>
<h2>Purchases</h2>
<p>Payment is processed by Stripe, acting as merchant of record and as controller of its own processing (see Stripe's privacy policy). Stripe sends us order and subscription identifiers, their status and your email address. The address is only used to send your key; we keep only a fingerprint of it, so we can resend your keys if you lose them. Legal basis: performance of the contract. Order data is kept as long as the law requires.</p>
<h2>Processors</h2>
<ul>
<li>Cloudflare (hosting of the website and the licence server), which keeps technical logs (IP address) for security.</li>
<li>Resend (sending the emails that contain keys).</li>
</ul>
<p>These providers are based in the United States; transfers rely on the EU–US Data Privacy Framework or on the European Commission's standard contractual clauses.</p>
<h2>Claude Code and Anthropic</h2>
<p>The app runs Claude Code with your own Anthropic account. What Claude Code sends to Anthropic falls under your relationship with Anthropic; we have no access to it.</p>
<h2>Your rights</h2>
<p>You can ask for access, rectification, erasure, restriction or portability of your data, or object to its processing, by writing to <a href="mailto:${v.email}">${v.email}</a>. You can also complain to the CNIL (cnil.fr) or your local authority.</p>`,
  },
  licence: {
    title: "Licence",
    body: `
<h2>Source code</h2>
<p>The code of ${v.product} is public, under the <a href="${v.source}">Functional Source License (FSL-1.1-ALv2)</a>: you may read, use, modify and redistribute it, including for professional use, except to offer a competing product. Two years after its release, each version becomes available under the Apache-2.0 licence.</p>
<h2>Pro licence</h2>
<p>Buying a Pro licence gives you a personal, non-exclusive, non-transferable right to use the Pro features on up to 3 computers, for the length of your subscription or for life depending on the offer. Professional use, including for your clients, is allowed.</p>
<p>You may not resell or publicly share your key, or circumvent the licence check.</p>
<p>When a subscription ends or after a refund, the app goes back to the free version. Your projects, history and settings stay on your computer.</p>
<p>The software comes with the statutory guarantees that apply to digital content, and no other warranty. The French version prevails.</p>`,
  },
});

export function legalPage(lang, id, vars) {
  return (lang === "fr" ? fr : en)(vars)[id];
}

// Early access (nothing sold, no account, no server): a short privacy page without personal
// identity, and the licence page. Contact goes through the public repository.
export const EARLY_PAGES = ["privacy", "licence"];

const earlyFr = (v) => ({
  privacy: {
    title: "Confidentialité",
    body: `
<p>Dernière mise à jour : ${v.updated}.</p>
<h2>Aucune donnée collectée</h2>
<p>Pendant l'accès anticipé, ${v.product} ne collecte aucune donnée personnelle : pas de compte, pas de paiement, pas de mesure d'audience, pas de cookie. L'app fonctionne entièrement sur ton ordinateur et ne contacte aucun serveur à nous.</p>
<h2>Claude Code et Anthropic</h2>
<p>L'app lance Claude Code avec ton propre compte Anthropic. Ce que Claude Code envoie à Anthropic relève de ta relation avec Anthropic ; nous n'y avons pas accès.</p>
<h2>Hébergement</h2>
<p>Ce site et les téléchargements sont hébergés par GitHub, qui peut conserver des journaux techniques (adresse IP) pour la sécurité, selon sa propre politique de confidentialité.</p>
<h2>Contact</h2>
<p>Pour toute question : <a href="${v.issues}">ouvre un ticket sur GitHub</a>.</p>`,
  },
  licence: fr(v).licence,
});

const earlyEn = (v) => ({
  privacy: {
    title: "Privacy",
    body: `
<p>Last updated: ${v.updated}.</p>
<h2>No data collected</h2>
<p>During early access, ${v.product} collects no personal data: no account, no payment, no analytics, no cookies. The app runs entirely on your computer and contacts no server of ours.</p>
<h2>Claude Code and Anthropic</h2>
<p>The app runs Claude Code with your own Anthropic account. What Claude Code sends to Anthropic falls under your relationship with Anthropic; we have no access to it.</p>
<h2>Hosting</h2>
<p>This website and the downloads are hosted by GitHub, which may keep technical logs (IP address) for security, under its own privacy policy.</p>
<h2>Contact</h2>
<p>For any question: <a href="${v.issues}">open an issue on GitHub</a>.</p>`,
  },
  licence: en(v).licence,
});

export function earlyPage(lang, id, vars) {
  return (lang === "fr" ? earlyFr : earlyEn)(vars)[id];
}