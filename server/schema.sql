-- Licence server database (Cloudflare D1). Personal data kept to the minimum:
-- no email address (only a hash, to resend keys), no card data (Stripe keeps it),
-- devices as anonymous hashes.

CREATE TABLE IF NOT EXISTS licences (
  id TEXT PRIMARY KEY,
  key_hash TEXT NOT NULL UNIQUE,
  key_enc TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('pro', 'lifetime')),
  status TEXT NOT NULL CHECK (status IN ('active', 'past_due', 'canceled', 'revoked')),
  period_end INTEGER,
  customer TEXT,
  subscription TEXT,
  payment_intent TEXT,
  session TEXT NOT NULL UNIQUE,
  email_hash TEXT,
  created INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS licences_subscription ON licences (subscription);
CREATE INDEX IF NOT EXISTS licences_payment_intent ON licences (payment_intent);
CREATE INDEX IF NOT EXISTS licences_customer ON licences (customer);
CREATE INDEX IF NOT EXISTS licences_email ON licences (email_hash);

CREATE TABLE IF NOT EXISTS activations (
  licence_id TEXT NOT NULL,
  device TEXT NOT NULL,
  first_seen INTEGER NOT NULL,
  last_seen INTEGER NOT NULL,
  PRIMARY KEY (licence_id, device)
);

CREATE TABLE IF NOT EXISTS trials (
  device TEXT PRIMARY KEY,
  started INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
  id TEXT PRIMARY KEY,
  created INTEGER NOT NULL
);
