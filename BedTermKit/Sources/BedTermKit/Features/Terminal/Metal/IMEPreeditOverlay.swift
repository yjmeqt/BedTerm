import UIKit

/// Floating overlay shown near the terminal cursor while the IME is composing
/// (Pinyin, Japanese Romaji, Korean 2-set, etc.). Hosted by
/// `TerminalMetalUIView` and driven by its `UITextInput` conformance.
final class IMEPreeditOverlay: UIView {
    private let label = UILabel()

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        layer.cornerRadius = 4
        layer.borderWidth = 1
        layer.borderColor = UIColor(named: "ShadcnBorder", in: .module, compatibleWith: nil)?.cgColor
        backgroundColor = UIColor(named: "ShadcnBackground", in: .module, compatibleWith: nil)
        label.textColor = UIColor(named: "ShadcnPrimary", in: .module, compatibleWith: nil)
        label.numberOfLines = 1
        label.adjustsFontForContentSizeCategory = false
        addSubview(label)

        // Re-resolve the border CGColor on appearance change so dark/light
        // flips don't strand the old colour reference. Uses the iOS 17+ trait
        // change registration API.
        registerForTraitChanges(
            [UITraitUserInterfaceStyle.self]
        ) { (overlay: IMEPreeditOverlay, _: UITraitCollection) in
            overlay.layer.borderColor = UIColor(
                named: "ShadcnBorder",
                in: .module,
                compatibleWith: overlay.traitCollection
            )?.cgColor
        }
    }

    required init?(coder: NSCoder) { nil }

    func setText(_ text: String, font: UIFont) {
        label.font = font
        label.text = text
        setNeedsLayout()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let insets = UIEdgeInsets(top: 2, left: 6, bottom: 2, right: 6)
        label.frame = bounds.inset(by: insets)
    }

    override func sizeThatFits(_ size: CGSize) -> CGSize {
        let textSize = label.sizeThatFits(size)
        return CGSize(width: ceil(textSize.width) + 12, height: ceil(textSize.height) + 4)
    }
}
