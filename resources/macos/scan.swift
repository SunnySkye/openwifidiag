import CoreWLAN
import Foundation

guard let interface = CWWiFiClient.shared().interface() else { exit(1) }
do {
    let networks = try interface.scanForNetworks(withName: nil)
    let rows: [[String: Any]] = networks.map { network in
        ["ssid": network.ssid ?? "<hidden>",
         "bssid": network.bssid ?? "",
         "rssi": network.rssiValue,
         "channel": network.wlanChannel?.channelNumber ?? 0,
         "description": network.description]
    }
    FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: rows))
} catch {
    FileHandle.standardError.write(Data("CoreWLAN scan failed: \(error)\n".utf8))
    exit(1)
}
