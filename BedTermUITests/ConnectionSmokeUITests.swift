import XCTest

final class ConnectionSmokeUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func test_launch_showsHostsThenFormFromAddButton() throws {
        let app = XCUIApplication()
        app.launchArguments.append("-uitest-skipOnboarding")
        app.launch()

        // Wait for the hosts root view to mount.
        XCTAssertTrue(
            app.descendants(matching: .any).matching(identifier: "hosts.root").firstMatch
                .waitForExistence(timeout: 8),
            "hosts.root should mount after -uitest-skipOnboarding"
        )

        // First-run shortcut may push the form automatically; if not, tap
        // + on the hosts list.
        if !app.textFields["connection.host"].waitForExistence(timeout: 3) {
            XCTAssertTrue(
                app.buttons["hosts.add"].waitForExistence(timeout: 3),
                "hosts.add button should be visible"
            )
            app.buttons["hosts.add"].tap()
            XCTAssertTrue(
                app.textFields["connection.host"].waitForExistence(timeout: 5),
                "connection form should appear after tapping +"
            )
        }
        XCTAssertTrue(app.textFields["connection.port"].exists)
        XCTAssertTrue(app.textFields["connection.username"].exists)
        XCTAssertTrue(app.buttons["connection.save"].exists)
    }
}
