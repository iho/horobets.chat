package messenger.app

import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import org.json.JSONObject

class MessagingService: FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        MobileNotifications.initialize(applicationContext)
        MobileNotifications.newToken(token)
    }
    override fun onMessageReceived(message: RemoteMessage) {
        MobileNotifications.initialize(applicationContext)
        val id = message.data["message_id"] ?: return
        MobileNotifications.show(message.data["title"] ?: message.notification?.title ?: "New message", message.data["body"] ?: message.notification?.body ?: "Open horobets.chat to read it", id, JSONObject(message.data).toString())
    }
}
