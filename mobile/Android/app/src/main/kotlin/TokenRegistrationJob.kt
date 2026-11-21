package messenger.app

import android.app.job.JobParameters
import android.app.job.JobService
import chat.horobets.core.MobileClient
import chat.horobets.core.SnapshotListener
import org.json.JSONObject
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** Token rollover can happen before an Activity exists. Reuse Rust auth/session handling. */
class TokenRegistrationJob: JobService() {
    private var client: MobileClient? = null
    private val stopped = AtomicBoolean(false)
    override fun onStartJob(params: JobParameters): Boolean {
        val prefs = getSharedPreferences("message-notifications", MODE_PRIVATE)
        val directory = prefs.getString("dataDirectory", null)
        val device = prefs.getString("deviceId", null)
        val token = params.extras.getString("token")
        if (!prefs.getBoolean("enabled", false) || directory == null || device == null || token == null) return false
        stopped.set(false)
        Thread {
            val done = CountDownLatch(1)
            val succeeded = AtomicBoolean(false)
            try {
                val engine = MobileClient(dataDir = directory)
                client = engine
                engine.subscribe(object: SnapshotListener {
                    override fun onSnapshot(snapshotJson: String) {
                        val snapshot = JSONObject(snapshotJson)
                        if (snapshot.optString("notice").startsWith("Push")) { succeeded.set(true); done.countDown() }
                        else if (!snapshot.isNull("error")) done.countDown()
                    }
                })
                engine.sendCommand(JSONObject().put("type", "register_device").put("payload", JSONObject()
                    .put("device_id", device).put("platform", "android").put("token", token).put("environment", "production")).toString())
                done.await(25, TimeUnit.SECONDS)
                if (!prefs.getBoolean("enabled", false)) engine.sendCommand(JSONObject().put("type", "unregister_device").put("payload", device).toString())
            } catch (_: Exception) {
                // JobScheduler retries network/service failures; credentials and tokens are never logged.
            } finally {
                client?.shutdown(); client = null
                if (!stopped.get()) jobFinished(params, !succeeded.get() && prefs.getBoolean("enabled", false))
            }
        }.start()
        return true
    }
    override fun onStopJob(params: JobParameters): Boolean {
        stopped.set(true); client?.shutdown(); client = null
        return true
    }
}
