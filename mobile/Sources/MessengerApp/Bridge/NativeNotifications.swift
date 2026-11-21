#if !SKIP && os(iOS)
import Foundation
import UIKit
import UserNotifications

@MainActor final class NativeNotifications: NSObject, UNUserNotificationCenterDelegate {
    static let shared = NativeNotifications()
    var tokenHandler: ((String, String) -> Void)?
    var openHandler: ((String) -> Void)?
    private var pendingRoute: String?
    private var delivered: Set<String> = []
    private var active = false
    private var workspace = ""
    private var channel = ""
    func install() { UNUserNotificationCenter.current().delegate = self }
    func callbacks(token: @escaping (String, String) -> Void, open: @escaping (String) -> Void) {
        tokenHandler = token; openHandler = open
        if let route = pendingRoute { pendingRoute = nil; open(route) }
    }
    func enable(completion: @escaping (Bool, String) -> Void) {
        install()
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
            Task { @MainActor in
                completion(granted, error?.localizedDescription ?? (granted ? "Notifications enabled" : "Notifications are blocked. Allow them in iOS Settings."))
                if granted { UIApplication.shared.registerForRemoteNotifications() }
            }
        }
    }
    func refresh() {
        if UserDefaults.standard.bool(forKey: "notificationsEnabled") { install(); UIApplication.shared.registerForRemoteNotifications() }
    }
    func registered(_ data: Data) {
        guard UserDefaults.standard.bool(forKey: "notificationsEnabled") else { return }
        let environment = Bundle.main.object(forInfoDictionaryKey: "HorobetsAPNSEnvironment") as? String ?? "development"
        tokenHandler?(data.map { String(format: "%02x", $0) }.joined(), environment)
    }
    func disable() { UIApplication.shared.unregisterForRemoteNotifications(); UNUserNotificationCenter.current().removeAllDeliveredNotifications() }
    func clearRead(workspace: String, unreadChannels: Set<String>) {
        UNUserNotificationCenter.current().getDeliveredNotifications { notifications in
            let ids = notifications.filter {
                let info = $0.request.content.userInfo
                return info["workspace_id"] as? String == workspace && !unreadChannels.contains(info["channel_id"] as? String ?? "")
            }.map { $0.request.identifier }
            UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ids)
        }
    }
    func reading(_ active: Bool, workspace: String, channel: String) { self.active = active; self.workspace = workspace; self.channel = channel }
    func show(title: String, body: String, id: String, route: String) {
        guard UserDefaults.standard.bool(forKey: "notificationsEnabled"), let data = route.data(using: .utf8), let info = try? JSONSerialization.jsonObject(with: data) as? [String: String] else { return }
        let content = UNMutableNotificationContent(); content.title = title; content.body = body; content.sound = .default; content.userInfo = info
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: id, content: content, trigger: nil))
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification, withCompletionHandler completionHandler: @escaping @Sendable (UNNotificationPresentationOptions) -> Void) {
        let info = notification.request.content.userInfo
        let w = info["workspace_id"] as? String ?? "", ch = info["channel_id"] as? String ?? ""
        let id = info["message_id"] as? String ?? notification.request.identifier
        Task { @MainActor in
            let reading = self.active && self.workspace == w && self.channel == ch
            let duplicate = self.delivered.contains(id)
            if self.delivered.count > 512 { self.delivered = [] }
            self.delivered.insert(id)
            completionHandler(!UserDefaults.standard.bool(forKey: "notificationsEnabled") || reading || duplicate ? [] : [.banner, .sound])
        }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse, withCompletionHandler completionHandler: @escaping @Sendable () -> Void) {
        let raw = try? JSONSerialization.data(withJSONObject: response.notification.request.content.userInfo)
        let route = raw.flatMap { String(data: $0, encoding: .utf8) }
        Task { @MainActor in
            if let route { if let handler = self.openHandler { handler(route) } else { self.pendingRoute = route } }
            completionHandler()
        }
    }
}
#endif
