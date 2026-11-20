import Foundation
import OSLog
import SwiftUI

/// A logger for the MessengerApp module.
let logger: Logger = Logger(subsystem: "chat.horobets.mobile", category: "MessengerApp")

/// The shared top-level view for the app, loaded from the platform-specific App delegates below.
///
/// The default implementation merely loads the `ContentView` for the app and logs a message.
public struct MessengerAppRootView : View {
    public init() {
    }

    public var body: some View {
        ContentView()
    }
}

/// Global application delegate functions.
///
/// These functions can update a shared observable object to communicate app state changes to interested views.
public final class MessengerAppAppDelegate : Sendable {
    public static let shared = MessengerAppAppDelegate()

    private init() {
    }

    public func onInit() {
        logger.debug("onInit")
    }

    public func onLaunch() {
        logger.debug("onLaunch")
    }

    public func onResume() {
        logger.debug("onResume")
    }

    public func onPause() {
        logger.debug("onPause")
    }

    public func onStop() {
        logger.debug("onStop")
    }

    public func onDestroy() {
        logger.debug("onDestroy")
    }

    public func onLowMemory() {
        logger.debug("onLowMemory")
    }
}
