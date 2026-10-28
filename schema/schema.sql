-- Main Postgres schema, shared by the backend and the indexer.
-- IDs are generated in Rust (uuid v4) to match the Prisma-style cuid columns.

CREATE TABLE IF NOT EXISTS "User" (
  id TEXT PRIMARY KEY,
  email TEXT UNIQUE NOT NULL,
  password TEXT NOT NULL,
  "publicKey" TEXT,
  "createdAt" TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
  "updatedAt" TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS "User_publicKey_idx" ON "User" ("publicKey");

CREATE TABLE IF NOT EXISTS "Asset" (
  id TEXT PRIMARY KEY,
  "mintAddress" TEXT UNIQUE NOT NULL,
  decimals INTEGER NOT NULL,
  name TEXT,
  symbol TEXT,
  "logoUrl" TEXT,
  "createdAt" TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
  "updatedAt" TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS "Balance" (
  id TEXT PRIMARY KEY,
  amount BIGINT NOT NULL DEFAULT 0,
  "createdAt" TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
  "updatedAt" TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
  "userId" TEXT NOT NULL REFERENCES "User"(id) ON DELETE CASCADE,
  "assetId" TEXT NOT NULL REFERENCES "Asset"(id) ON DELETE CASCADE,
  UNIQUE("userId", "assetId")
);

CREATE INDEX IF NOT EXISTS "Balance_userId_idx" ON "Balance" ("userId");
CREATE INDEX IF NOT EXISTS "Balance_assetId_idx" ON "Balance" ("assetId");

-- MPC server schema (one database per MPC node, never the main database).
-- Each node only stores its own share of the user's key; the full key never
-- exists in one place. `mpc` also creates this table on startup so a fresh
-- node database works without a migration step.
CREATE TABLE IF NOT EXISTS keyshares (
  user_id TEXT PRIMARY KEY,
  "publicKey" TEXT NOT NULL,
  "privateKey" TEXT NOT NULL
);
