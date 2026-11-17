# axo Steer Reliability & Queue Control (Verified)

- **Status:** Adopted & verified on `axo` `a0ec20d53e253e6e4bea33d9c969f76f4399b872`.
- **Target repository:** `/repo/rust/axo`
- **Verified revision:** `a0ec20d53e253e6e4bea33d9c969f76f4399b872` (`feat(axo): let hosts see, withdraw, and release queued input`)
- **Related plan row:** [`../plan.md`](../plan.md#t10-remaining-axo-gaps) ([T10](../plan.md#t10-remaining-axo-gaps))

---

## Verified in `axo` `a0ec20d`

All pinned symptoms from `4d6bc34` / `b961066` are resolved in `a0ec20d`:

1. **Stop / Failed `auto_paused` trap fixed (`session/mod.rs`, `session/delivery.rs`):**
   - `Session::abort()` sets `auto_paused = true` when pressed during an active run; `SessionHooks::settle()` only sets `auto_paused = true` when the outcome is neither `Done` nor `Aborted(CancelReason::User)`.
   - Input sent *after* pressing Stop (`prompt`, `steer`, `follow_up`, or host `Steer`/`FollowUp`) clears `auto_paused` in `queue_message()` and starts the next run as soon as the stopped run settles.
   - Input queued *before* Stop or when a run fails stays held in `LiveSnapshot::queued` with `auto_paused = true` until `resume_wakes()` or new input arrives.
   - `Session::resume_wakes()` now clears `auto_paused` and calls `claim_idle()` -> `release_or_deliver()`, so held steers/follow-ups run immediately (ahead of pending job wakes/mail).
2. **Pending `ask` & reviewed `yield` no longer block steers (`tools/ask.rs`, `session/modes.rs`, `config.rs`):**
   - Both `AskTool` and `ModeYield` check `steer.is_pending()` and `tokio::select!` on `steer.pending()`.
   - A queued steer settles `ask` (`SessionEvent::QuestionResolved`, `details.outcome == "steered"`) and reviewed `yield` (`ResultResolution::Steered`, `ModeResultResolved`, `details.outcome == "steered"`, keeping the mode selected), delivering the steer immediately after.
   - `Settings::default().steering_mode` changed from `OneAtATime` to `All`.
3. **Host queue visibility & withdrawal (`session/mod.rs`, `session/delivery.rs`, `store/entry.rs`, `agent.rs`):**
   - `LiveSnapshot::queued: Vec<QueuedInput>` replaces `queued_steering` / `queued_follow_up`, returning `{ submission_id, kind, parts }` in exact delivery order (steers, then follow-ups; reopened `SubmissionKind::Prompt` behaves like `Steer`).
   - `Session::withdraw(&self, submission_id: &str) -> Result<Vec<PromptPart>, Error>` removes the queued entry from `Agent`, persists `Entry::SubmissionWithdrawn`, and returns its original `Vec<PromptPart>` (leaving `PromptPart::Saved` intact on disk). Returns `Error::NotQueued` if already delivered or withdrawn.

