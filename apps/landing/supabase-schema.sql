-- NexusMind Waitlist Table
-- Run this SQL in your Supabase SQL Editor (Dashboard > SQL Editor)

CREATE TABLE IF NOT EXISTS waitlist (
  id BIGSERIAL PRIMARY KEY,
  name TEXT NOT NULL,
  email TEXT NOT NULL,
  company TEXT NOT NULL,
  team_size TEXT NOT NULL DEFAULT '51-200',
  interests TEXT[] DEFAULT '{}',
  message TEXT DEFAULT '',
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Enable Row Level Security
ALTER TABLE waitlist ENABLE ROW LEVEL SECURITY;

-- Policy: allow anon insert only (we don't want anon reads!)
CREATE POLICY "Allow anonymous inserts" ON waitlist
  FOR INSERT
  TO anon
  WITH CHECK (true);

-- Policy: only authenticated users can read
CREATE POLICY "Allow authenticated reads" ON waitlist
  FOR SELECT
  TO authenticated
  USING (true);

-- Index for time-based queries
CREATE INDEX idx_waitlist_created_at ON waitlist (created_at DESC);

-- Index for email dedup checks
CREATE INDEX idx_waitlist_email ON waitlist (email);
