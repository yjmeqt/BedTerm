import Darwin
import NIOCore

/// Enables TCP keepalive on the underlying socket as soon as the channel is
/// active, so an idle SSH session survives NAT/firewall idle timeouts and a
/// half-open connection is detected within a bounded window instead of
/// silently stranded.
///
/// On Apple platforms `TCP_KEEPALIVE` is the per-socket idle interval (seconds
/// before the first probe). We also set `TCP_KEEPINTVL` and `TCP_KEEPCNT` so a
/// dead peer is declared within roughly `idle + intvl × cnt` seconds.
final class TCPKeepaliveHandler: ChannelInboundHandler {
    typealias InboundIn = Any

    private let idleSeconds: Int32
    private let intervalSeconds: Int32
    private let probeCount: Int32

    init(idleSeconds: Int = 30, intervalSeconds: Int = 15, probeCount: Int = 3) {
        self.idleSeconds = Int32(idleSeconds)
        self.intervalSeconds = Int32(intervalSeconds)
        self.probeCount = Int32(probeCount)
    }

    func channelActive(context: ChannelHandlerContext) {
        // Best-effort: failures are non-fatal (e.g. on a non-TCP channel).
        let channel = context.channel
        _ = channel.setOption(ChannelOptions.socket(SocketOptionLevel(SOL_SOCKET), SO_KEEPALIVE), value: 1)
        Self.setIntOption(on: channel, level: IPPROTO_TCP, name: TCP_KEEPALIVE, value: idleSeconds)
        Self.setIntOption(on: channel, level: IPPROTO_TCP, name: TCP_KEEPINTVL, value: intervalSeconds)
        Self.setIntOption(on: channel, level: IPPROTO_TCP, name: TCP_KEEPCNT, value: probeCount)
        context.fireChannelActive()
    }

    private static func setIntOption(on channel: Channel, level: Int32, name: Int32, value: Int32) {
        _ = channel.setOption(
            ChannelOptions.socket(SocketOptionLevel(level), SocketOptionName(name)),
            value: SocketOptionValue(value)
        )
    }
}
