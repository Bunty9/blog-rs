-- Track the wall-clock moment a row was flipped from 'pending' to 'sending'
-- so a crash-mid-dispatch row can be reclaimed by a later worker tick.
--
-- Without this column, a process that returned from `mailer.send` but died
-- before `mark_sent` would leave the row stuck in 'sending' indefinitely
-- (silent under-delivery). The worker now runs `reclaim_stale` on every tick
-- and rotates rows older than OUTBOX_RECLAIM_AFTER back to 'pending'.

ALTER TABLE newsletter_outbox ADD COLUMN claimed_at INTEGER;
