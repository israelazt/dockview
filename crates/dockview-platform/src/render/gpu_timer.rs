//! Measuring how long the GPU spends on each frame, with timestamp queries.

use super::*;

/// Measures how long the GPU spends on each frame, using timestamp queries.
///
/// Results arrive a few frames late (the GPU runs behind the CPU), so queries
/// are kept in a small ring and read back without ever stalling.
pub(super) struct GpuTimer {
    pub(super) sets: Vec<QuerySet>,
    pub(super) next: usize,
    pub(super) active: Option<usize>,
    /// Smoothed GPU time per frame, in milliseconds.
    pub(super) smoothed_ms: Option<f32>,
}

pub(super) struct QuerySet {
    pub(super) disjoint: ID3D11Query,
    pub(super) start: ID3D11Query,
    pub(super) end: ID3D11Query,
    pub(super) in_flight: bool,
}

impl GpuTimer {
    const RING: usize = 4;

    pub(super) fn new(device: &ID3D11Device) -> Result<Self> {
        let make = |kind| -> Result<ID3D11Query> {
            let desc = D3D11_QUERY_DESC {
                Query: kind,
                MiscFlags: 0,
            };
            let mut query = None;
            // SAFETY: valid query description and out-pointer.
            unsafe { device.CreateQuery(&desc, Some(&mut query))? };
            query.ok_or_else(missing)
        };
        let mut sets = Vec::with_capacity(Self::RING);
        for _ in 0..Self::RING {
            sets.push(QuerySet {
                disjoint: make(D3D11_QUERY_TIMESTAMP_DISJOINT)?,
                start: make(D3D11_QUERY_TIMESTAMP)?,
                end: make(D3D11_QUERY_TIMESTAMP)?,
                in_flight: false,
            });
        }
        Ok(Self {
            sets,
            next: 0,
            active: None,
            smoothed_ms: None,
        })
    }

    pub(super) fn begin(&mut self, context: &ID3D11DeviceContext) {
        let index = self.next;
        if self.sets[index].in_flight {
            self.active = None; // the GPU is still behind: skip this frame
            return;
        }
        let set = &self.sets[index];
        // SAFETY: the queries belong to the same device as `context`.
        unsafe {
            context.Begin(&set.disjoint);
            context.End(&set.start);
        }
        self.active = Some(index);
    }

    pub(super) fn end(&mut self, context: &ID3D11DeviceContext) {
        let Some(index) = self.active.take() else {
            return;
        };
        let set = &mut self.sets[index];
        // SAFETY: see `begin`.
        unsafe {
            context.End(&set.end);
            context.End(&set.disjoint);
        }
        set.in_flight = true;
        self.next = (index + 1) % Self::RING;
    }

    /// Reads back every finished query set.
    pub(super) fn collect(&mut self, context: &ID3D11DeviceContext) {
        for set in self.sets.iter_mut().filter(|s| s.in_flight) {
            // GetData reports "not ready yet" as a success code, so a zero
            // frequency (the value we pre-load) means nothing was written.
            let mut disjoint = D3D11_QUERY_DATA_TIMESTAMP_DISJOINT::default();
            let (mut start, mut end) = (0u64, 0u64);
            // SAFETY: every out-pointer matches the size passed with it.
            let ready = unsafe {
                context
                    .GetData(
                        &set.disjoint,
                        Some((&mut disjoint as *mut D3D11_QUERY_DATA_TIMESTAMP_DISJOINT).cast()),
                        std::mem::size_of::<D3D11_QUERY_DATA_TIMESTAMP_DISJOINT>() as u32,
                        0,
                    )
                    .is_ok()
                    && disjoint.Frequency != 0
                    && context
                        .GetData(&set.start, Some((&mut start as *mut u64).cast()), 8, 0)
                        .is_ok()
                    && context
                        .GetData(&set.end, Some((&mut end as *mut u64).cast()), 8, 0)
                        .is_ok()
                    && end != 0
            };
            if !ready {
                continue;
            }
            set.in_flight = false;
            if !disjoint.Disjoint.as_bool() && end >= start {
                let ms = (end - start) as f64 / disjoint.Frequency as f64 * 1000.0;
                self.smoothed_ms = Some(match self.smoothed_ms {
                    Some(previous) => previous * 0.9 + ms as f32 * 0.1,
                    None => ms as f32,
                });
            }
        }
    }
}
