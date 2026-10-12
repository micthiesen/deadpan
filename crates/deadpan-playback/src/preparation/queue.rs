//! A single producer's bounded, generation-scoped canonical PCM handoff.
use std::collections::VecDeque;

use deadpan_diagnostics::{QUEUES, Share};

use super::{BATCH_FRAMES, Reply};

// Round down to complete batches: 175 * 8192 = 1,433,600 stereo frames
// (29.867 seconds / 10.9375 MiB). One producer and one controller batch, the
// native four-second packet queue, and canonical caches are separate bounds.
pub(crate) const CAPACITY: usize = 30 * 48_000 / BATCH_FRAMES;

pub(crate) struct Queue {
    items: VecDeque<Reply>,
    epoch: Option<u64>,
    terminal: bool,
    replies: Share,
    frames: Share,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
            epoch: None,
            terminal: false,
            replies: Share::new(&QUEUES.playback_prepared),
            frames: Share::new(&QUEUES.playback_prepared_frames),
        }
    }
}

impl Queue {
    pub(crate) fn is_full(&self) -> bool {
        self.terminal || self.items.len() >= CAPACITY
    }

    pub(crate) fn push(&mut self, reply: Reply) -> Result<(), String> {
        let (epoch, frames, terminal) = match &reply {
            Reply::Batch(batch) => {
                if batch.samples.capacity() > BATCH_FRAMES || self.items.len() >= CAPACITY {
                    return Err("prepared audio queue exceeded its PCM bound".into());
                }
                (batch.epoch, batch.samples.len() as u64, batch.eos)
            }
            // A failure has one reserved terminal slot, even with a full PCM
            // FIFO. Preserve the order of successfully prepared prefix audio.
            Reply::Failed { epoch, .. } => (*epoch, 0, true),
        };
        if self.terminal || self.epoch.is_some_and(|current| current != epoch) {
            return Err("prepared audio queue received a foreign or terminated stream".into());
        }
        self.epoch = Some(epoch);
        self.terminal = terminal;
        self.items.push_back(reply);
        self.replies.set(self.items.len() as u64);
        self.frames.set(self.frames.value() + frames);
        Ok(())
    }

    pub(crate) fn pop(&mut self) -> Option<Reply> {
        let reply = self.items.pop_front()?;
        if let Reply::Batch(batch) = &reply {
            self.frames
                .set(self.frames.value() - batch.samples.len() as u64);
        }
        self.replies.set(self.items.len() as u64);
        Some(reply)
    }

    pub(crate) fn clear(&mut self) {
        self.items.clear();
        self.epoch = None;
        self.terminal = false;
        self.replies.set(0);
        self.frames.set(0);
    }

    pub(crate) fn discard(&mut self, epoch: u64) {
        if self.epoch == Some(epoch) {
            self.clear();
        }
    }

    #[cfg(test)]
    pub(crate) fn frames(&self) -> u64 {
        self.frames.value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preparation::Batch;
    use deadpan_core::AudioSample;

    fn batch(index: usize, eos: bool) -> Reply {
        Reply::Batch(Batch {
            epoch: 1,
            start: AudioSample((index * BATCH_FRAMES) as i64),
            end: AudioSample(((CAPACITY + 1) * BATCH_FRAMES) as i64),
            samples: vec![[index as f32; 2]; BATCH_FRAMES],
            eos,
        })
    }

    #[test]
    fn bounded_fifo_retains_pcm_order_and_reserves_a_failure_after_a_full_prefix() {
        let mut queue = Queue::default();
        for index in 0..CAPACITY {
            queue.push(batch(index, false)).unwrap();
        }
        assert!(queue.is_full());
        assert_eq!(queue.frames(), (CAPACITY * BATCH_FRAMES) as u64);
        assert!(queue.push(batch(CAPACITY, false)).is_err());
        queue
            .push(Reply::Failed {
                epoch: 1,
                error: "future stage failed".into(),
            })
            .unwrap();
        assert!(
            queue
                .push(Reply::Failed {
                    epoch: 1,
                    error: "duplicate".into()
                })
                .is_err()
        );
        for index in 0..CAPACITY {
            let Some(Reply::Batch(got)) = queue.pop() else {
                panic!("missing prefix batch")
            };
            assert_eq!(got.start.0, (index * BATCH_FRAMES) as i64);
            assert!(
                got.samples
                    .iter()
                    .all(|sample| *sample == [index as f32; 2])
            );
        }
        assert_eq!(queue.frames(), 0);
        assert!(
            matches!(queue.pop(), Some(Reply::Failed { error, .. }) if error == "future stage failed")
        );
        assert!(queue.pop().is_none());
    }

    #[test]
    fn allocation_limits_epoch_and_end_are_enforced_until_clear() {
        let mut queue = Queue::default();
        let Reply::Batch(mut oversized) = batch(0, false) else {
            unreachable!()
        };
        oversized.samples.reserve_exact(1);
        assert!(queue.push(Reply::Batch(oversized)).is_err());
        queue.push(batch(0, true)).unwrap();
        assert!(queue.push(batch(1, false)).is_err());
        queue.discard(2);
        assert_eq!(queue.frames(), BATCH_FRAMES as u64);
        queue.discard(1);
        assert_eq!(queue.frames(), 0);
        queue
            .push(Reply::Failed {
                epoch: 2,
                error: "new epoch".into(),
            })
            .unwrap();
        queue.clear();
        queue.push(batch(0, false)).unwrap();
        assert!(
            queue
                .push(Reply::Failed {
                    epoch: 2,
                    error: "foreign".into()
                })
                .is_err()
        );
    }
}
