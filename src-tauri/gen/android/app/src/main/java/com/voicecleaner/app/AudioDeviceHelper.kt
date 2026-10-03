package com.voicecleaner.app

import android.app.Activity
import android.content.Context
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject

object AudioDeviceHelper {
    private const val TAG = "AudioDeviceHelper"

    private var audioManager: AudioManager? = null
    private var appContext: Context? = null
    private var manualSelectedDeviceId: Int? = null
    private var isRecordingActive: Boolean = false
    private var currentTargetDevice: AudioDeviceInfo? = null

    // Priority tiers: Wired External (0) > Bluetooth (1) > Internal (2)
    const val TIER_WIRED = 0
    const val TIER_BLUETOOTH = 1
    const val TIER_INTERNAL = 2

    data class DiscoveredDevice(
        val id: Int,
        val name: String,
        val tier: Int,
        val deviceInfo: AudioDeviceInfo,
        val isDefault: Boolean,
        val isSelected: Boolean
    )

    private var isCallbackRegistered = false
    private val deviceCallback = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(addedDevices: Array<out AudioDeviceInfo>?) {
            Log.i(TAG, "Audio devices added, re-evaluating priority routing")
            if (manualSelectedDeviceId == null) {
                selectDefaultDevice()
            }
        }

        override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>?) {
            Log.i(TAG, "Audio devices removed, re-evaluating priority routing")
            val selectedId = manualSelectedDeviceId
            if (selectedId != null) {
                val currentInputs = audioManager?.getDevices(AudioManager.GET_DEVICES_INPUTS) ?: emptyArray()
                if (currentInputs.none { it.id == selectedId }) {
                    manualSelectedDeviceId = null
                }
            }
            if (manualSelectedDeviceId == null) {
                selectDefaultDevice()
            }
        }
    }

    private var activityRef: java.lang.ref.WeakReference<Activity>? = null

    fun init(activity: Activity) {
        activityRef = java.lang.ref.WeakReference(activity)
        appContext = activity.applicationContext
        audioManager = activity.getSystemService(Context.AUDIO_SERVICE) as? AudioManager

        if (audioManager == null) {
            Log.e(TAG, "AudioManager not available")
            return
        }

        // Always keep media volume as the controlled stream
        activity.volumeControlStream = AudioManager.STREAM_MUSIC

        // Register dynamic plug/unplug callback once
        if (!isCallbackRegistered) {
            audioManager?.registerAudioDeviceCallback(deviceCallback, Handler(Looper.getMainLooper()))
            isCallbackRegistered = true
        }

        // Set default routing target without forcing MODE_IN_COMMUNICATION during idle
        selectDefaultDevice()
    }

    fun onPermissionsGranted() {
        Log.i(TAG, "Audio permissions granted, refreshing routing")
        selectDefaultDevice()
    }

    private fun getDeviceTier(device: AudioDeviceInfo): Int {
        val type = device.type
        val name = device.productName?.toString()?.lowercase() ?: ""
        return when {
            type == AudioDeviceInfo.TYPE_USB_HEADSET ||
            type == AudioDeviceInfo.TYPE_USB_DEVICE ||
            type == AudioDeviceInfo.TYPE_WIRED_HEADSET ||
            type == AudioDeviceInfo.TYPE_LINE_ANALOG ||
            type == AudioDeviceInfo.TYPE_LINE_DIGITAL ||
            name.contains("usb") || name.contains("wired") -> TIER_WIRED

            type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO ||
            type == AudioDeviceInfo.TYPE_BLE_HEADSET ||
            type == AudioDeviceInfo.TYPE_BLUETOOTH_A2DP ||
            name.contains("bluetooth") || name.contains("rockerz") || name.contains("earbuds") || name.contains("airpods") -> TIER_BLUETOOTH

            else -> TIER_INTERNAL
        }
    }

    /**
     * Curates and deduplicates raw hardware HAL endpoints into clean logical audio devices
     * matching the behavior of production audio applications like Dolby On.
     */
    fun listInputDevices(): List<DiscoveredDevice> {
        val am = audioManager ?: return emptyList()
        val allInputs = am.getDevices(AudioManager.GET_DEVICES_INPUTS)
        if (allInputs.isEmpty()) return emptyList()

        val list = mutableListOf<DiscoveredDevice>()

        // 1. Wired / USB External devices (Tier 0)
        val wiredDevs = allInputs.filter { getDeviceTier(it) == TIER_WIRED }
        val wiredGrouped = wiredDevs.groupBy { it.productName?.toString()?.trim() ?: "Wired Audio" }
        for ((rawName, group) in wiredGrouped) {
            val rep = group.first()
            val friendlyName = when {
                rep.type == AudioDeviceInfo.TYPE_USB_HEADSET || rep.type == AudioDeviceInfo.TYPE_USB_DEVICE -> {
                    if (rawName.isNotEmpty() && !rawName.equals("USB Audio", ignoreCase = true)) {
                        "USB Microphone ($rawName)"
                    } else {
                        "USB Microphone"
                    }
                }
                rep.type == AudioDeviceInfo.TYPE_WIRED_HEADSET -> {
                    if (rawName.isNotEmpty() && !rawName.equals("headset", ignoreCase = true)) {
                        "Wired Headset ($rawName)"
                    } else {
                        "Wired Headset (3.5mm)"
                    }
                }
                else -> if (rawName.isNotEmpty()) "Wired Audio ($rawName)" else "Wired Audio"
            }
            list.add(
                DiscoveredDevice(
                    id = rep.id,
                    name = friendlyName,
                    tier = TIER_WIRED,
                    deviceInfo = rep,
                    isDefault = false,
                    isSelected = false
                )
            )
        }

        // 2. Bluetooth Headsets (Tier 1) - collapse duplicate SCO/BLE/A2DP HAL endpoints for the same headset
        val btDevs = allInputs.filter { getDeviceTier(it) == TIER_BLUETOOTH }
        val btGrouped = btDevs.groupBy { it.productName?.toString()?.trim() ?: "Bluetooth Headset" }
        for ((rawName, group) in btGrouped) {
            // Prefer BLE or SCO as representative capture endpoint
            val rep = group.find { it.type == AudioDeviceInfo.TYPE_BLE_HEADSET }
                ?: group.find { it.type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO }
                ?: group.first()
            val friendlyName = if (rawName.contains("Bluetooth", ignoreCase = true)) {
                rawName
            } else {
                "$rawName (Bluetooth)"
            }
            list.add(
                DiscoveredDevice(
                    id = rep.id,
                    name = friendlyName,
                    tier = TIER_BLUETOOTH,
                    deviceInfo = rep,
                    isDefault = false,
                    isSelected = false
                )
            )
        }

        // 3. Built-in Internal Microphones (Tier 2) - collapse all bottom, top, camera mics into ONE Phone Microphone
        val internalMics = allInputs.filter { getDeviceTier(it) == TIER_INTERNAL }
        if (internalMics.isNotEmpty()) {
            val primaryMic = internalMics.find { it.type == AudioDeviceInfo.TYPE_BUILTIN_MIC }
                ?: internalMics.first()
            list.add(
                DiscoveredDevice(
                    id = primaryMic.id,
                    name = "Phone Microphone (Built-in)",
                    tier = TIER_INTERNAL,
                    deviceInfo = primaryMic,
                    isDefault = false,
                    isSelected = false
                )
            )
        }

        // Sort by priority tier: Wired (0) > Bluetooth (1) > Internal (2)
        list.sortBy { it.tier }

        val activeSelectedId = manualSelectedDeviceId ?: list.firstOrNull()?.id

        return list.mapIndexed { index, item ->
            item.copy(
                isDefault = (index == 0),
                isSelected = (item.id == activeSelectedId)
            )
        }
    }

    @JvmStatic
    fun getDevicesJson(): String {
        return try {
            val devices = listInputDevices()
            val array = JSONArray()
            for (dev in devices) {
                val obj = JSONObject()
                obj.put("id", dev.id)
                obj.put("name", dev.name)
                obj.put("tier", dev.tier)
                obj.put("is_default", dev.isDefault)
                obj.put("is_selected", dev.isSelected)
                array.put(obj)
            }
            array.toString()
        } catch (e: Exception) {
            Log.e(TAG, "Error generating devices JSON: ${e.message}", e)
            "[]"
        }
    }

    @JvmStatic
    fun selectDevice(id: Int): Boolean {
        val am = audioManager ?: return false
        val allInputs = am.getDevices(AudioManager.GET_DEVICES_INPUTS)
        val target = allInputs.find { it.id == id } ?: return false

        manualSelectedDeviceId = id
        currentTargetDevice = target
        return if (isRecordingActive) {
            applyActiveRouting(target)
        } else {
            true
        }
    }

    @JvmStatic
    fun selectDefaultDevice(): Boolean {
        manualSelectedDeviceId = null
        val devices = listInputDevices()
        val top = devices.firstOrNull() ?: return false
        currentTargetDevice = top.deviceInfo
        return if (isRecordingActive) {
            applyActiveRouting(top.deviceInfo)
        } else {
            true
        }
    }

    /**
     * Called when recording or calibration starts.
     * Engages Bluetooth communication device routing only if a Bluetooth device is selected,
     * leaving Built-in / Wired microphones in pure MODE_NORMAL without call mode.
     */
    @JvmStatic
    fun onRecordingStarted(): Boolean {
        isRecordingActive = true
        val target = currentTargetDevice ?: listInputDevices().firstOrNull()?.deviceInfo ?: return false
        currentTargetDevice = target
        return applyActiveRouting(target)
    }

    /**
     * Called when recording or calibration stops.
     * Releases communication routing and restores MODE_NORMAL, ensuring the volume rocker
     * always controls Media Volume.
     */
    @JvmStatic
    fun onRecordingStopped(): Boolean {
        isRecordingActive = false
        val am = audioManager ?: return false
        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                am.clearCommunicationDevice()
            } else {
                @Suppress("DEPRECATION")
                if (am.isBluetoothScoOn) {
                    am.stopBluetoothSco()
                    am.isBluetoothScoOn = false
                }
            }
            am.mode = AudioManager.MODE_NORMAL
            activityRef?.get()?.volumeControlStream = AudioManager.STREAM_MUSIC
            Log.i(TAG, "Recording stopped: restored MODE_NORMAL and STREAM_MUSIC")
            true
        } catch (e: Exception) {
            Log.e(TAG, "Error in onRecordingStopped: ${e.message}", e)
            false
        }
    }

    private fun applyActiveRouting(device: AudioDeviceInfo): Boolean {
        val am = audioManager ?: return false
        val tier = getDeviceTier(device)
        val isBt = (tier == TIER_BLUETOOTH)
        Log.i(TAG, "applyActiveRouting: target=${device.productName} (id=${device.id}, isBt=$isBt)")

        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                if (isBt) {
                    am.mode = AudioManager.MODE_IN_COMMUNICATION
                    val commDevices = am.availableCommunicationDevices
                    val match = commDevices.find { it.id == device.id }
                        ?: commDevices.find { it.productName?.toString().equals(device.productName?.toString(), ignoreCase = true) }
                        ?: commDevices.find { it.type == device.type }
                        ?: commDevices.find { it.type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO || it.type == AudioDeviceInfo.TYPE_BLE_HEADSET }
                        ?: device
                    val res = am.setCommunicationDevice(match)
                    Log.i(TAG, "setCommunicationDevice(${match.productName}, id=${match.id}) result=$res")
                    res
                } else {
                    am.clearCommunicationDevice()
                    am.mode = AudioManager.MODE_NORMAL
                    activityRef?.get()?.volumeControlStream = AudioManager.STREAM_MUSIC
                    true
                }
            } else {
                @Suppress("DEPRECATION")
                if (isBt) {
                    am.mode = AudioManager.MODE_IN_COMMUNICATION
                    am.startBluetoothSco()
                    am.isBluetoothScoOn = true
                    Log.i(TAG, "startBluetoothSco requested")
                    true
                } else {
                    if (am.isBluetoothScoOn) {
                        am.stopBluetoothSco()
                        am.isBluetoothScoOn = false
                    }
                    am.mode = AudioManager.MODE_NORMAL
                    true
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to apply audio routing: ${e.message}", e)
            false
        }
    }
}
