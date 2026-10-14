//! Cancellation-aware receives below TLS for the locked ureq 3.4 transport API.
//! Never retry writes: a failed `write_all` can have transmitted a partial TLS record.
use std::fmt;
use std::time::{Duration, Instant};

use lightcraft_sysmem::jobs::Cancelled;
use ureq::unversioned::resolver::{DefaultResolver, Resolver};
use ureq::unversioned::transport::time::Duration as WireDuration;
use ureq::unversioned::transport::{Buffers, ConnectProxyConnector, ConnectionDetails, Connector, NextTimeout, RustlsConnector, TcpConnector, Transport};
use ureq::{Error, ProxyProtocol, Timeout};

use crate::JobControl;

const RECEIVE_SLICE: Duration = Duration::from_millis(200);
const SETUP_LIMIT: Duration = Duration::from_secs(20);

pub(super) fn agent(config: ureq::config::Config, control: &JobControl) -> ureq::Agent {
    agent_with_resolver(config, control, DefaultResolver::default())
}

pub(super) fn agent_with_resolver(config: ureq::config::Config, control: &JobControl, resolver: impl Resolver) -> ureq::Agent {
    // CONNECT recursively invokes this chain for its proxy socket. Only fresh TCP
    // transports are wrapped; never wrap a completed tunnel/TLS stream a second time.
    let connector = ConnectProxyConnector::default().chain(CancelTcp { control: control.clone() }).chain(RustlsConnector::default());
    ureq::Agent::with_parts(config, connector, resolver)
}

fn cancelled(control: &JobControl) -> Result<(), Error> {
    if control.is_cancelled() { Err(std::io::Error::other(Cancelled).into()) } else { Ok(()) }
}

struct CancelTcp {
    control: JobControl,
}
impl fmt::Debug for CancelTcp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancelTcp").finish_non_exhaustive()
    }
}
impl<In: Transport> Connector<In> for CancelTcp {
    type Out = Box<dyn Transport>;
    fn connect(&self, details: &ConnectionDetails, chained: Option<In>) -> Result<Option<Self::Out>, Error> {
        cancelled(&self.control)?;
        if details
            .config
            .proxy()
            .is_some_and(|proxy| !proxy.is_no_proxy(details.uri) && !matches!(proxy.protocol(), ProxyProtocol::Http | ProxyProtocol::Https))
        {
            return Err(std::io::Error::other("This build does not support SOCKS download proxies.").into());
        }
        if let Some(transport) = chained {
            return Ok(Some(Box::new(transport)));
        }
        let setup_deadline = Instant::now() + (*details.timeout.after).min(SETUP_LIMIT);
        let transport = <TcpConnector as Connector<()>>::connect(&TcpConnector::default(), details, None)?;
        cancelled(&self.control)?;
        Ok(transport.map(|inner| {
            Box::new(CancelTransport { inner: Box::new(inner), control: self.control.clone(), setup_deadline, deadlines: Vec::new(), failed: false })
                as Box<dyn Transport>
        }))
    }
}

struct CancelTransport {
    inner: Box<dyn Transport>,
    control: JobControl,
    setup_deadline: Instant,
    // One request per socket (idle pooling is disabled). A stale timeout passed
    // repeatedly by TLS must tighten, never extend, the same phase's deadline.
    deadlines: Vec<(Timeout, Instant)>,
    failed: bool,
}
impl fmt::Debug for CancelTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancelTransport").field("failed", &self.failed).finish_non_exhaustive()
    }
}
impl CancelTransport {
    fn deadline(&mut self, next: NextTimeout) -> Instant {
        let proposed = match next.after {
            WireDuration::Exact(after) => Instant::now().checked_add(after).unwrap_or(self.setup_deadline),
            // CONNECT request writes have no timeout in ureq 3.4. Bound them by
            // the socket's setup deadline instead of allowing an unbounded write.
            WireDuration::NotHappening => self.setup_deadline,
        };
        let proposed = if next.reason == Timeout::Connect { proposed.min(self.setup_deadline) } else { proposed };
        if let Some((_, deadline)) = self.deadlines.iter_mut().find(|(reason, _)| *reason == next.reason) {
            *deadline = (*deadline).min(proposed);
            *deadline
        } else {
            self.deadlines.push((next.reason, proposed));
            proposed
        }
    }
    fn remaining(&mut self, deadline: Instant, reason: Timeout) -> Result<Duration, Error> {
        cancelled(&self.control)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            self.failed = true;
            Err(Error::Timeout(reason))
        } else {
            Ok(remaining)
        }
    }
}
impl Transport for CancelTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn transmit_output(&mut self, amount: usize, next: NextTimeout) -> Result<(), Error> {
        let deadline = self.deadline(next);
        let remaining = self.remaining(deadline, next.reason)?.min(SETUP_LIMIT);
        // Forward exactly once, even on timeout after partial transmission.
        let result = self.inner.transmit_output(amount, NextTimeout { after: WireDuration::Exact(remaining), reason: next.reason });
        if result.is_err() {
            self.failed = true;
        }
        cancelled(&self.control)?;
        result
    }
    fn maybe_await_input(&mut self, next: NextTimeout) -> Result<bool, Error> {
        cancelled(&self.control)?;
        if self.inner.buffers().can_use_input() { Ok(true) } else { self.await_input(next) }
    }
    fn await_input(&mut self, next: NextTimeout) -> Result<bool, Error> {
        let deadline = self.deadline(next);
        loop {
            let remaining = self.remaining(deadline, next.reason)?;
            let sliced = NextTimeout { after: WireDuration::Exact(remaining.min(RECEIVE_SLICE)), reason: next.reason };
            let result = self.inner.await_input(sliced);
            cancelled(&self.control)?;
            match result {
                Err(Error::Timeout(reason)) if reason == next.reason => continue,
                Err(e) => {
                    self.failed = true;
                    return Err(e);
                }
                Ok(progress) => return Ok(progress),
            }
        }
    }
    fn is_open(&mut self) -> bool {
        !self.failed && !self.control.is_cancelled() && self.inner.is_open()
    }
    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use ureq::unversioned::transport::LazyBuffers;

    #[derive(Debug)]
    struct Probe {
        buffers: LazyBuffers,
        reads: Arc<AtomicUsize>,
        writes: Arc<AtomicUsize>,
    }
    impl Transport for Probe {
        fn buffers(&mut self) -> &mut dyn Buffers {
            &mut self.buffers
        }
        fn transmit_output(&mut self, _: usize, next: NextTimeout) -> Result<(), Error> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            Err(Error::Timeout(next.reason))
        }
        fn await_input(&mut self, next: NextTimeout) -> Result<bool, Error> {
            if self.reads.fetch_add(1, Ordering::SeqCst) < 2 { Err(Error::Timeout(next.reason)) } else { Ok(true) }
        }
        fn is_open(&mut self) -> bool {
            true
        }
    }
    fn probe() -> (CancelTransport, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let reads = Arc::new(AtomicUsize::new(0));
        let writes = Arc::new(AtomicUsize::new(0));
        (
            CancelTransport {
                inner: Box::new(Probe { buffers: LazyBuffers::new(16, 16), reads: reads.clone(), writes: writes.clone() }),
                control: JobControl::new(),
                setup_deadline: Instant::now() + SETUP_LIMIT,
                deadlines: Vec::new(),
                failed: false,
            },
            reads,
            writes,
        )
    }
    #[test]
    fn artificial_receive_timeouts_retry_but_partial_writes_never_replay() {
        let (mut transport, reads, writes) = probe();
        let next = NextTimeout { after: WireDuration::from_secs(1), reason: Timeout::RecvBody };
        assert!(transport.await_input(next).unwrap());
        assert_eq!(reads.load(Ordering::SeqCst), 3);
        assert!(transport.transmit_output(8, next).is_err());
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert!(!transport.is_open());
    }
    #[test]
    fn stale_tls_fragment_timeouts_cannot_extend_phase_deadline() {
        let (mut transport, _, _) = probe();
        let next = NextTimeout { after: WireDuration::from_millis(80), reason: Timeout::RecvBody };
        let first = transport.deadline(next);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(transport.deadline(next), first);
        transport.deadlines[0].1 = Instant::now();
        assert!(matches!(transport.await_input(next), Err(Error::Timeout(Timeout::RecvBody))));
    }
    #[test]
    fn tls_connect_cannot_restart_the_budget_after_tcp() {
        let (mut transport, reads, _) = probe();
        transport.setup_deadline = Instant::now();
        let next = NextTimeout { after: WireDuration::from_secs(20), reason: Timeout::Connect };
        assert!(matches!(transport.await_input(next), Err(Error::Timeout(Timeout::Connect))));
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        let (mut transport, _, _) = probe();
        transport.setup_deadline = Instant::now() + Duration::from_millis(10);
        assert_eq!(transport.deadline(next), transport.setup_deadline);
    }
}
