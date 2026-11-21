package messenger.app

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging
import org.json.JSONObject

/** OS notification permissions, delivery and taps. No chat sync or auth tokens here. */
object MobileNotifications {
    private var context: Context? = null
    private var permission: ActivityResultLauncher<String>? = null
    private var enabledCallback: ((Boolean, String) -> Unit)? = null
    private var tokenCallback: ((String, String) -> Unit)? = null
    private var openCallback: ((String) -> Unit)? = null
    private var statusCallback: ((String) -> Unit)? = null
    private var pendingRoute: String? = null
    private var active = false
    private var readingWorkspace = ""
    private var readingChannel = ""
    private fun prefs(c: Context) = c.getSharedPreferences("message-notifications", Context.MODE_PRIVATE)
    fun install(owner: ComponentActivity) {
        initialize(owner.applicationContext)
        permission = owner.registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted -> finishEnable(granted) }
        openIntent(owner.intent)
    }
    fun initialize(c: Context) {
        context = c.applicationContext
        c.getSystemService(NotificationManager::class.java).createNotificationChannel(NotificationChannel("messages", "Messages", NotificationManager.IMPORTANCE_DEFAULT))
        FirebaseApp.initializeApp(c)
    }
    fun callbacks(token: (String, String) -> Unit, open: (String) -> Unit, status: (String) -> Unit) {
        tokenCallback = token; openCallback = open; statusCallback = status
        pendingRoute?.let { pendingRoute = null; open(it) }
        refresh()
    }
    fun account(user: String) {
        val c = context ?: return
        if (prefs(c).getString("user", "") != user) {
            NotificationManagerCompat.from(c).cancelAll()
            prefs(c).edit().putString("user", user).apply()
        }
    }
    fun reading(isActive: Boolean, workspace: String, channel: String) {
        active = isActive; readingWorkspace = workspace; readingChannel = channel
    }
    fun enable(completion: (Boolean, String) -> Unit) {
        enabledCallback = completion
        val c = context ?: return completion(false, "Notification service is unavailable")
        if (Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(c, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            permission?.launch(Manifest.permission.POST_NOTIFICATIONS) ?: completion(false, "Reopen the app to request notification permission")
        } else finishEnable(NotificationManagerCompat.from(c).areNotificationsEnabled())
    }
    private fun finishEnable(granted: Boolean) {
        val c = context ?: return
        prefs(c).edit().putBoolean("enabled", granted).apply()
        val configured = FirebaseApp.getApps(c).isNotEmpty()
        enabledCallback?.invoke(granted, if (!granted) "Notifications are blocked. Allow them in Android app settings." else if (!configured) "Live alerts enabled. Firebase configuration is needed for background push." else "Notifications enabled")
        enabledCallback = null
        if (granted) refresh()
    }
    fun refresh() {
        val c = context ?: return
        if (!prefs(c).getBoolean("enabled", false) || FirebaseApp.getApps(c).isEmpty()) return
        FirebaseMessaging.getInstance().token.addOnSuccessListener { token -> newToken(token) }
            .addOnFailureListener { statusCallback?.invoke("Could not register background push. Check Firebase configuration and your connection.") }
    }
    fun configure(dataDirectory: String, deviceId: String) {
        val c = context ?: return
        prefs(c).edit().putString("dataDirectory", dataDirectory).putString("deviceId", deviceId).apply()
    }
    fun newToken(token: String) {
        val c = context ?: return
        if (!prefs(c).getBoolean("enabled", false)) return
        val callback = tokenCallback
        if (callback != null) { callback(token, "production"); return }
        // A token can rotate while Firebase starts a cold process without any UI.
        // Schedule a short native-core registration job rather than losing it.
        val extras = android.os.PersistableBundle().apply { putString("token", token) }
        val component = android.content.ComponentName(c.packageName, "messenger.app.TokenRegistrationJob")
        c.getSystemService(android.app.job.JobScheduler::class.java).schedule(
            android.app.job.JobInfo.Builder(0x48434E41, component).setRequiredNetworkType(android.app.job.JobInfo.NETWORK_TYPE_ANY)
                .setBackoffCriteria(30_000L, android.app.job.JobInfo.BACKOFF_POLICY_EXPONENTIAL).setExtras(extras).build())
    }
    fun disable() {
        val c = context ?: return
        prefs(c).edit().putBoolean("enabled", false).apply()
        c.getSystemService(android.app.job.JobScheduler::class.java).cancel(0x48434E41)
        NotificationManagerCompat.from(c).cancelAll()
        if (FirebaseApp.getApps(c).isNotEmpty()) FirebaseMessaging.getInstance().deleteToken()
    }
    fun show(title: String, body: String, id: String, route: String) {
        val c = context ?: return
        if (!prefs(c).getBoolean("enabled", false) || !NotificationManagerCompat.from(c).areNotificationsEnabled()) return
        val data = JSONObject(route)
        if (data.optString("user_id") != prefs(c).getString("user", "")) return
        if (active && data.optString("workspace_id") == readingWorkspace && data.optString("channel_id") == readingChannel) return
        val intent = c.packageManager.getLaunchIntentForPackage(c.packageName) ?: return
        intent.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP).putExtra("horobets_route", route)
        val pending = PendingIntent.getActivity(c, id.hashCode(), intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val icon = c.resources.getIdentifier("ic_notification", "drawable", c.packageName).takeIf { it != 0 } ?: android.R.drawable.stat_notify_chat
        val metadata = android.os.Bundle().apply { putString("workspace_id", data.optString("workspace_id")); putString("channel_id", data.optString("channel_id")) }
        val notification = NotificationCompat.Builder(c, "messages").addExtras(metadata).setSmallIcon(icon).setContentTitle(title).setContentText(body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body)).setContentIntent(pending).setAutoCancel(true)
            .setOnlyAlertOnce(true).setVisibility(NotificationCompat.VISIBILITY_PRIVATE).build()
        NotificationManagerCompat.from(c).notify(id, 0, notification)
    }
    fun clearRead(workspace: String, unreadChannels: String) {
        val c = context ?: return
        val array = org.json.JSONArray(unreadChannels)
        val unread = (0 until array.length()).map { array.getString(it) }.toSet()
        val manager = c.getSystemService(NotificationManager::class.java)
        manager.activeNotifications.filter {
            it.notification.extras.getString("workspace_id") == workspace && it.notification.extras.getString("channel_id") !in unread
        }.forEach { manager.cancel(it.tag, it.id) }
    }
    fun openIntent(intent: Intent?) {
        val c = context ?: return
        val route = intent?.getStringExtra("horobets_route") ?: intent?.extras?.let { extras ->
            val message = extras.getString("message_id") ?: return@let null
            JSONObject().put("workspace_id", extras.getString("workspace_id")).put("channel_id", extras.getString("channel_id"))
                .put("message_id", message).put("user_id", extras.getString("user_id")).toString()
        } ?: return
        val user = JSONObject(route).optString("user_id")
        if (user.isBlank() || user != prefs(c).getString("user", "")) return
        intent?.removeExtra("horobets_route"); intent?.removeExtra("message_id")
        val handler = openCallback
        if (handler == null) pendingRoute = route else handler(route)
    }
}
