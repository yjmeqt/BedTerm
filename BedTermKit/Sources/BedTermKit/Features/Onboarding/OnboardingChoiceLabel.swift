import SwiftUI

/// Shared card-style choice label used by onboarding picker steps.
struct OnboardingChoiceLabel: View {
    let title: LocalizedStringKey
    let subtitle: LocalizedStringKey

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.headline)
                Text(subtitle).font(.subheadline).foregroundStyle(.secondary)
            }
            Spacer()
            Image(systemName: "chevron.right").foregroundStyle(.tertiary)
        }
        .padding()
        .background(
            RoundedRectangle(cornerRadius: 14)
                .fill(.regularMaterial)
        )
    }
}
