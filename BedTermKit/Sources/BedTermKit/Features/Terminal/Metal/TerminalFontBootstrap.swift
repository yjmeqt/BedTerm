import BedTermCoreC
import CoreText
import UIKit

/// Wire iOS's monospace font into the Rust rasterizer at app launch.
///
/// Try-order:
///   1. **Menlo** — the only host monospace we register. It's iOS's
///      reliable named monospace (resolvable by PostScript name through
///      `UIFont(name:)` on every iOS version), renders cleanly through
///      swash (the Rust rasterizer), and is what Xcode and Terminal.app
///      shipped with pre-2018. We extract its bytes via `CTFontCopyTable`
///      sfnt reassembly rather than trusting `kCTFontURLAttribute`,
///      since the on-disk URL isn't always readable from the app sandbox.
///   2. No-op — Rust falls through to its default font cascade (there
///      is no bundled JetBrains Mono anymore).
///
/// The Rust side stores both the registered face bytes and the chosen
/// family name; cosmic-text requests `Family::Name(...)` against that,
/// so a successful registration here flips the entire rasterizer to
/// the host font with no further plumbing.
@MainActor
enum TerminalFontBootstrap {
    @discardableResult
    static func registerHostMonospace() -> String? {
        guard let menlo = UIFont(name: "Menlo-Regular", size: 14),
            let chosen = tryRegister(font: menlo)
        else {
            return nil
        }
        // Companion faces. cosmic-text matches `Attrs::weight` / `style`
        // against fontdb after the family lookup, so loading these lets
        // SGR 1 (bold) / SGR 3 (italic) terminal cells actually pick the
        // right cut at shape time. Each is best-effort — if iOS ever
        // drops one, the cell just falls back to Regular without crashing.
        registerAux(name: "Menlo-Bold")
        registerAux(name: "Menlo-Italic")
        registerAux(name: "Menlo-BoldItalic")
        return chosen
    }

    /// Look up a named UIFont, reassemble its sfnt bytes, push as an
    /// auxiliary face (does NOT touch the primary `TERMINAL_FAMILY`).
    private static func registerAux(name: String) {
        guard let font = UIFont(name: name, size: 14),
            let data = sfntData(from: font as CTFont),
            !data.isEmpty
        else {
            return
        }
        _ = data.withUnsafeBytes { bytes -> Int32 in
            guard let base = bytes.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return -1
            }
            return bt_font_register_aux_face(base, UInt(data.count))
        }
    }

    private static func tryRegister(font: UIFont) -> String? {
        let ctFont = font as CTFont
        guard let data = sfntData(from: ctFont), !data.isEmpty else {
            return nil
        }
        // The Rust side reads the resolved family back out of fontdb
        // (the only authoritative source for `Family::Name(...)`
        // matching at shape time) and writes it into our buffer. 128
        // bytes is a comfortable upper bound — real family names top
        // out around 30 chars.
        var nameBuf = [UInt8](repeating: 0, count: 128)
        let written: Int32 = data.withUnsafeBytes { bytes in
            guard let base = bytes.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return Int32(-1)
            }
            return nameBuf.withUnsafeMutableBufferPointer { outBuf -> Int32 in
                guard let outBase = outBuf.baseAddress else { return -1 }
                return bt_font_register_terminal_face(
                    base, UInt(data.count), outBase, UInt(outBuf.count))
            }
        }
        guard written >= 0 else { return nil }
        let len = min(Int(written), nameBuf.count)
        return String(bytes: nameBuf.prefix(len), encoding: .utf8)
    }

    /// Rebuild an sfnt (TTF / OpenType) byte stream from a `CTFont` by
    /// enumerating every table tag and concatenating with a fresh
    /// header + table directory. Works for any CTFont whose tables are
    /// readable — including system UI fonts (SF Mono) whose on-disk
    /// file path the sandbox blocks.
    ///
    /// Spec: https://docs.microsoft.com/typography/opentype/spec/otff
    private static func sfntData(from ctFont: CTFont) -> Data? {
        let tables = copyTables(from: ctFont)
        guard !tables.isEmpty else { return nil }
        let entries = buildDirectory(for: tables)
        return assemble(tables: tables, entries: entries)
    }

    private static func copyTables(from ctFont: CTFont) -> [(tag: UInt32, data: Data)] {
        let opts = CTFontTableOptions(rawValue: 0)
        guard let tagsArr = CTFontCopyAvailableTables(ctFont, opts) else { return [] }
        let count = CFArrayGetCount(tagsArr)
        guard count > 0 else { return [] }

        var tables: [(tag: UInt32, data: Data)] = []
        tables.reserveCapacity(count)
        for idx in 0..<count {
            let raw = CFArrayGetValueAtIndex(tagsArr, idx)
            // CFArrayGetValueAtIndex returns the value as an untyped
            // pointer; for the CTFontTableTag array each "pointer" is
            // actually a uint32 stuffed into the slot.
            let tag = UInt32(UInt(bitPattern: raw) & 0xFFFF_FFFF)
            guard let cfData = CTFontCopyTable(ctFont, tag, opts) else { continue }
            tables.append((tag, cfData as Data))
        }
        return tables
    }

    private static func buildDirectory(for tables: [(tag: UInt32, data: Data)]) -> [DirEntry] {
        let headerSize = 12
        let dirSize = 16 * tables.count
        var bodyOffset = headerSize + dirSize
        var entries: [DirEntry] = []
        entries.reserveCapacity(tables.count)
        for (tag, raw) in tables {
            let length = raw.count
            let pad = (4 - length % 4) % 4
            entries.append(
                DirEntry(
                    tag: tag,
                    checksum: tableChecksum(raw),
                    offset: UInt32(bodyOffset),
                    length: UInt32(length)))
            bodyOffset += length + pad
        }
        return entries
    }

    private static func assemble(
        tables: [(tag: UInt32, data: Data)],
        entries: [DirEntry]
    ) -> Data {
        let cffTag: UInt32 = 0x4346_4620  // 'CFF '
        let cff2Tag: UInt32 = 0x4346_4632  // 'CFF2'
        let hasCFF = tables.contains { $0.tag == cffTag || $0.tag == cff2Tag }
        let sfntVersion: UInt32 = hasCFF ? 0x4F54_544F : 0x0001_0000  // 'OTTO' / TTF

        let numTables = UInt16(tables.count)
        var entrySelector: UInt16 = 0
        var probe = numTables
        while probe > 1 {
            probe >>= 1
            entrySelector += 1
        }
        let searchRange = UInt16(1 << entrySelector) * 16
        let rangeShift = numTables * 16 - searchRange

        var out = Data()
        out.appendBE(sfntVersion)
        out.appendBE(numTables)
        out.appendBE(searchRange)
        out.appendBE(entrySelector)
        out.appendBE(rangeShift)
        for entry in entries {
            out.appendBE(entry.tag)
            out.appendBE(entry.checksum)
            out.appendBE(entry.offset)
            out.appendBE(entry.length)
        }
        for (_, raw) in tables {
            out.append(raw)
            let pad = (4 - raw.count % 4) % 4
            if pad > 0 {
                out.append(Data(repeating: 0, count: pad))
            }
        }
        return out
    }

    private struct DirEntry {
        let tag: UInt32
        let checksum: UInt32
        let offset: UInt32
        let length: UInt32
    }

    /// OpenType table checksum: sum of all 4-byte longs, big-endian,
    /// trailing bytes zero-padded. Most font parsers don't validate
    /// this strictly, but emitting a real value avoids the brittle
    /// edge of meeting parsers that do.
    private static func tableChecksum(_ data: Data) -> UInt32 {
        var sum: UInt32 = 0
        let count = data.count
        data.withUnsafeBytes { raw in
            guard let ptr = raw.bindMemory(to: UInt8.self).baseAddress else { return }
            var off = 0
            while off + 4 <= count {
                let b0 = UInt32(ptr[off])
                let b1 = UInt32(ptr[off + 1])
                let b2 = UInt32(ptr[off + 2])
                let b3 = UInt32(ptr[off + 3])
                sum &+= (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
                off += 4
            }
            if off < count {
                var word: UInt32 = 0
                var shift: UInt32 = 24
                while off < count {
                    word |= UInt32(ptr[off]) << shift
                    shift &-= 8
                    off += 1
                }
                sum &+= word
            }
        }
        return sum
    }
}

extension Data {
    fileprivate mutating func appendBE(_ value: UInt32) {
        var be = value.bigEndian
        Swift.withUnsafeBytes(of: &be) { self.append(contentsOf: $0) }
    }
    fileprivate mutating func appendBE(_ value: UInt16) {
        var be = value.bigEndian
        Swift.withUnsafeBytes(of: &be) { self.append(contentsOf: $0) }
    }
}
