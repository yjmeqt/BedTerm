import XCTest

final class ConnectionSmokeUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func test_launch_showsConnectionScreen() throws {
        let app = XCUIApplication()
        app.launchArguments.append("-uitest-skipOnboarding")
        app.launch()
        XCTAssertTrue(app.textFields["connection.host"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.textFields["connection.port"].exists)
        XCTAssertTrue(app.textFields["connection.username"].exists)
        XCTAssertTrue(app.buttons["connection.connect"].exists)
    }
}
