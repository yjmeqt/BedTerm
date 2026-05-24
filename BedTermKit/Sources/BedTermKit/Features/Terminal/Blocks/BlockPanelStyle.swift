import CoreGraphics

/// Tunables for the block list's layout. We dropped the Warp-style
/// rounded panel chrome in favour of a divider-separated list — blocks
/// share the scroll background; only a hairline `ShadcnBorder` line
/// sits in the gap between them.
@MainActor
enum BlockPanelStyle {
    /// Vertical gap between adjacent blocks. Blocks now butt against
    /// each other; the hairline divider above each header is the only
    /// thing separating one block from the next, so a non-zero gap
    /// would read as an unexplained vacant strip.
    static let interBlockGapPt: CGFloat = 0

    /// Horizontal inset between the panel's left edge and cell column 0.
    /// Kept so the Metal text indents nicely from the screen edge even
    /// though there is no longer a panel BG.
    static let cellLeftInsetPt: CGFloat = 12

    /// Hairline thickness drawn in the gap between blocks.
    static let dividerThicknessPt: CGFloat = 1

    /// Horizontal inset for the divider hairline — slightly indented
    /// from the screen edge so it reads as a list separator rather
    /// than a hard rule.
    static let dividerHorizontalInsetPt: CGFloat = 16
}
