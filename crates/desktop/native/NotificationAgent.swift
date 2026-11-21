import AppKit
import Foundation
import UserNotifications

func emit(_ event: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: event), let line = String(data: data, encoding: .utf8) else { return }
    print(line); fflush(stdout)
}
final class Agent: NSObject, UNUserNotificationCenterDelegate {
    func command(_ value: [String: Any]) {
        guard Bundle.main.bundleIdentifier != nil else { emit(["type":"error","message":"Build and open the desktop app bundle to enable notifications: scripts/build-desktop-app.sh"]); return }
        if value["type"] as? String == "status" { emit(["type":"status","bundle":Bundle.main.bundleIdentifier ?? "","path":Bundle.main.bundleURL.path]); return }
        let center = UNUserNotificationCenter.current(); center.delegate = self
        switch value["type"] as? String {
        case "authorize":
            center.requestAuthorization(options: [.alert,.sound,.badge]) { granted, error in
                emit(["type":"permission","granted":granted,"message":error?.localizedDescription ?? (granted ? "Notifications enabled" : "Allow notifications in macOS System Settings.")])
            }
        case "notify":
            let content=UNMutableNotificationContent()
            content.title=value["title"] as? String ?? "New message"
            content.body=value["body"] as? String ?? ""
            content.sound = .default
            content.userInfo=value["route"] as? [String:String] ?? [:]
            center.add(UNNotificationRequest(identifier:value["id"] as? String ?? UUID().uuidString,content:content,trigger:nil)) { error in
                if let error { emit(["type":"error","message":error.localizedDescription]) }
            }
        case "clear_read":
            let workspace=value["workspace"] as? String ?? ""
            let unread=Set(value["channels"] as? [String] ?? [])
            center.getDeliveredNotifications { notifications in
                let ids=notifications.filter { n in n.request.content.userInfo["workspace_id"] as? String == workspace && !unread.contains(n.request.content.userInfo["channel_id"] as? String ?? "") }.map { $0.request.identifier }
                center.removeDeliveredNotifications(withIdentifiers:ids)
            }
        case "clear": center.removeAllDeliveredNotifications()
        default: break
        }
    }
    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification, withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions)->Void) {
        completionHandler([.banner,.sound])
    }
    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse, withCompletionHandler completionHandler: @escaping ()->Void) {
        emit(["type":"open","route":response.notification.request.content.userInfo])
        let app=Bundle.main.bundleURL
        if app.pathExtension == "app" { NSWorkspace.shared.openApplication(at:app,configuration:NSWorkspace.OpenConfiguration(),completionHandler:nil) }
        completionHandler()
    }
}
let agent=Agent()
DispatchQueue.global().async {
    while let line=readLine() {
        if let data=line.data(using:.utf8),let value=(try? JSONSerialization.jsonObject(with:data)) as? [String:Any] {
            DispatchQueue.main.async { agent.command(value) }
        }
    }
    exit(0)
}
RunLoop.main.run()
