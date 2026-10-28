-- Main Postgres schema (Prisma-style)
CREATE TABLE IF NOT EXISTS "User" (
  id TEXT PRIMARY KEY,
  email TEXT UNIQUE NOT NULL,
  password TEXT NOT NULL,
  "publicKey" TEXT,
  "createdAt" TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
  "updatedAt" TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

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

-- MPC server schema (per MPC instance)
CREATE TABLE IF NOT EXISTS keyshares (
  user_id TEXT PRIMARY KEY,
  "publicKey" TEXT NOT NULL,
  "privateKey" TEXT NOT NULL
);
