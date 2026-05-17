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

        // First-run shortcut may push the form automatically; if not, the user
        // taps + on the empty Hosts list.
        if !app.textFields["connection.host"].waitForExistence(timeout: 3) {
            app.buttons["hosts.add"].tap()
            XCTAssertTrue(app.textFields["connection.host"].waitForExistence(timeout: 5))
        }
        XCTAssertTrue(app.textFields["connection.port"].exists)
        XCTAssertTrue(app.textFields["connection.username"].exists)
        XCTAssertTrue(app.buttons["connection.save"].exists)
    }
}
