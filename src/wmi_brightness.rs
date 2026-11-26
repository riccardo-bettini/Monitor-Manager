use windows::core::{w, BSTR, VARIANT};
use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE,
};

const RPC_C_AUTHN_WINNT: u32 = 10;
const RPC_C_AUTHZ_NONE: u32 = 0;

#[derive(Debug, Clone)]
pub struct WmiBrightnessPanel {
    pub instance_name: String,
    pub current: u32,
    pub max: u32,
}

struct ComGuard {
    should_uninit: bool,
}

impl ComGuard {
    fn init() -> Result<Self, ()> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr == S_OK || hr == S_FALSE {
                Ok(Self { should_uninit: true })
            } else if hr == RPC_E_CHANGED_MODE {
                Ok(Self {
                    should_uninit: false,
                })
            } else {
                Err(())
            }
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.should_uninit {
            unsafe { CoUninitialize() };
        }
    }
}

#[derive(Debug, Clone)]
pub struct WmiMonitorIdentity {
    pub instance_name: String,
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
}

/// Sessione WMI riutilizzabile sullo stesso thread worker per aggiornamenti instant (<1ms).
/// IMPORTANTE: l'ordine dei campi determina l'ordine di Drop in Rust!
/// `services` (IWbemServices) DEVE essere dichiarato PRIMA di `_com` (ComGuard),
/// così `IWbemServices::Release()` viene chiamato prima di `CoUninitialize()`.
pub struct WmiSession {
    services: IWbemServices,
    _com: ComGuard,
}

impl WmiSession {
    pub fn connect() -> Option<Self> {
        let com = ComGuard::init().ok()?;
        let services = unsafe {
            let locator: IWbemLocator =
                CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).ok()?;
            let services = locator
                .ConnectServer(
                    &BSTR::from("ROOT\\WMI"),
                    &BSTR::new(),
                    &BSTR::new(),
                    &BSTR::new(),
                    0,
                    &BSTR::new(),
                    None,
                )
                .ok()?;

            let _ = CoSetProxyBlanket(
                &services,
                RPC_C_AUTHN_WINNT,
                RPC_C_AUTHZ_NONE,
                None,
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            );
            services
        };
        Some(Self {
            services,
            _com: com,
        })
    }

    pub fn set_brightness(&self, target_instance: &str, level: u32) -> bool {
        let clamped = level.min(100) as u8;
        unsafe {
            let mut class_obj: Option<IWbemClassObject> = None;
            if self
                .services
                .GetObject(
                    &BSTR::from("WmiMonitorBrightnessMethods"),
                    Default::default(),
                    None,
                    Some(&mut class_obj),
                    None,
                )
                .is_err()
            {
                return false;
            }
            let Some(class_obj) = class_obj else {
                return false;
            };

            let mut in_sig: Option<IWbemClassObject> = None;
            if class_obj
                .GetMethod(w!("WmiSetBrightness"), 0, &mut in_sig, std::ptr::null_mut())
                .is_err()
            {
                return false;
            }
            let Some(in_sig) = in_sig else {
                return false;
            };

            let Ok(in_params) = in_sig.SpawnInstance(0) else {
                return false;
            };
            if in_params
                .Put(w!("Timeout"), 0, &VARIANT::from(0u32), 0)
                .is_err()
            {
                return false;
            }
            if in_params
                .Put(w!("Brightness"), 0, &VARIANT::from(clamped), 0)
                .is_err()
            {
                return false;
            }

            let Ok(enumerator) = self.services.ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from("SELECT InstanceName, __RELPATH FROM WmiMonitorBrightnessMethods"),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            ) else {
                return false;
            };

            let mut applied = false;
            loop {
                let mut objs: [Option<IWbemClassObject>; 1] = [None];
                let mut returned = 0u32;
                let hr = enumerator.Next(WBEM_INFINITE, &mut objs, &mut returned);
                if hr.is_err() || returned == 0 {
                    break;
                }
                if let Some(obj) = objs[0].take() {
                    let mut v_inst = VARIANT::default();
                    let mut v_rel = VARIANT::default();
                    if obj.Get(w!("InstanceName"), 0, &mut v_inst, None, None).is_ok()
                        && obj.Get(w!("__RELPATH"), 0, &mut v_rel, None, None).is_ok()
                    {
                        if let (Ok(inst_bstr), Ok(rel_bstr)) =
                            (BSTR::try_from(&v_inst), BSTR::try_from(&v_rel))
                        {
                            let inst = inst_bstr.to_string();
                            if target_instance.is_empty()
                                || inst.eq_ignore_ascii_case(target_instance)
                            {
                                if self
                                    .services
                                    .ExecMethod(
                                        &rel_bstr,
                                        &BSTR::from("WmiSetBrightness"),
                                        Default::default(),
                                        None,
                                        &in_params,
                                        None,
                                        None,
                                    )
                                    .is_ok()
                                {
                                    applied = true;
                                }
                                if !target_instance.is_empty() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            applied
        }
    }
}

/// Normalizza un ID PnP hardware per confrontare WMI `InstanceName` (`DISPLAY\AUOD2A2\..._0`)
/// con EDID `monitorDevicePath` (`\\?\DISPLAY#AUOD2A2#...#{guid}`).
pub fn matches_device_path(wmi_instance: &str, edid_device_path: &str) -> bool {
    if wmi_instance.is_empty() || edid_device_path.is_empty() {
        return false;
    }

    let norm_wmi = wmi_instance.to_uppercase().replace('#', "\\");
    let norm_wmi_trimmed = norm_wmi
        .rsplit_once('_')
        .map(|(head, _)| head)
        .unwrap_or(&norm_wmi);

    let norm_edid = edid_device_path.to_uppercase().replace('#', "\\");

    if norm_edid.contains(norm_wmi_trimmed) {
        return true;
    }

    let wmi_parts: Vec<&str> = norm_wmi.split('\\').collect();
    let edid_parts: Vec<&str> = norm_edid.split('\\').collect();
    if wmi_parts.len() >= 2 && edid_parts.len() >= 3 {
        let wmi_pnp = wmi_parts[1];
        if !wmi_pnp.is_empty() && edid_parts.iter().any(|p| *p == wmi_pnp) {
            return true;
        }
    }

    false
}

/// Interroga tutti i pannelli portatili integrati supportati da Windows WMI (`root\WMI`).
pub fn query_wmi_panels() -> Vec<WmiBrightnessPanel> {
    let mut panels = Vec::new();
    let Some(session) = WmiSession::connect() else {
        return panels;
    };

    unsafe {
        if let Ok(enumerator) = session.services.ExecQuery(
            &BSTR::from("WQL"),
            &BSTR::from("SELECT InstanceName, CurrentBrightness FROM WmiMonitorBrightness"),
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        ) {
            loop {
                let mut objs: [Option<IWbemClassObject>; 1] = [None];
                let mut returned = 0u32;
                let hr = enumerator.Next(WBEM_INFINITE, &mut objs, &mut returned);
                if hr.is_err() || returned == 0 {
                    break;
                }
                if let Some(obj) = objs[0].take() {
                    let mut v_inst = VARIANT::default();
                    let mut v_cur = VARIANT::default();
                    if obj.Get(w!("InstanceName"), 0, &mut v_inst, None, None).is_ok()
                        && obj
                            .Get(w!("CurrentBrightness"), 0, &mut v_cur, None, None)
                            .is_ok()
                    {
                        let current_opt: Result<u32, ()> = u32::try_from(&v_cur)
                            .or_else(|_| i32::try_from(&v_cur).map(|v| v.max(0) as u32))
                            .map_err(|_| ())
                            .or_else(|_| {
                                let raw = v_cur.as_raw();
                                if raw.Anonymous.Anonymous.vt == 17 /* VT_UI1 */ {
                                    Ok(raw.Anonymous.Anonymous.Anonymous.bVal as u32)
                                } else {
                                    Err(())
                                }
                            });
                        if let (Ok(inst_bstr), Ok(cur)) = (BSTR::try_from(&v_inst), current_opt) {
                            panels.push(WmiBrightnessPanel {
                                instance_name: inst_bstr.to_string(),
                                current: cur.min(100),
                                max: 100,
                            });
                        }
                    }
                }
            }
        }
    }

    panels
}

unsafe fn decode_wmi_u16_array(v: &VARIANT) -> String {
    use windows::Win32::System::Ole::{
        SafeArrayAccessData, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayUnaccessData,
    };

    let raw = v.as_raw();
    let vt = raw.Anonymous.Anonymous.vt;
    const VT_ARRAY_FLAG: u16 = 0x2000;
    if (vt & VT_ARRAY_FLAG) == 0 {
        return String::new();
    }
    let elem_vt = vt & !VT_ARRAY_FLAG;
    let psa = raw.Anonymous.Anonymous.Anonymous.parray
        as *const windows::Win32::System::Com::SAFEARRAY;
    if psa.is_null() {
        return String::new();
    }

    let Ok(lb) = SafeArrayGetLBound(psa, 1) else {
        return String::new();
    };
    let Ok(ub) = SafeArrayGetUBound(psa, 1) else {
        return String::new();
    };
    if ub < lb {
        return String::new();
    }
    let count = (ub - lb + 1) as usize;
    if count == 0 || count > 256 {
        return String::new();
    }

    let mut data_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    if SafeArrayAccessData(psa, &mut data_ptr).is_err() || data_ptr.is_null() {
        return String::new();
    }

    let mut u16_chars = Vec::with_capacity(count);
    match elem_vt {
        // VT_I4 (3) or VT_UI4 (19): WMI often marshals uint16[] as 32-bit ints
        3 | 19 => {
            let slice = std::slice::from_raw_parts(data_ptr as *const u32, count);
            for &val in slice {
                if val == 0 {
                    break;
                }
                u16_chars.push(val as u16);
            }
        }
        // VT_I2 (2) or VT_UI2 (18)
        2 | 18 => {
            let slice = std::slice::from_raw_parts(data_ptr as *const u16, count);
            for &val in slice {
                if val == 0 {
                    break;
                }
                u16_chars.push(val);
            }
        }
        // VT_UI1 (17) | VT_I1 (16)
        16 | 17 => {
            let slice = std::slice::from_raw_parts(data_ptr as *const u8, count);
            for &val in slice {
                if val == 0 {
                    break;
                }
                u16_chars.push(val as u16);
            }
        }
        _ => {}
    }

    let _ = SafeArrayUnaccessData(psa);
    String::from_utf16_lossy(&u16_chars).trim().to_string()
}

/// Interroga produttore, modello e numero di serie EDID dei monitor tramite `WmiMonitorID` (`root\WMI`).
pub fn query_wmi_identities() -> Vec<WmiMonitorIdentity> {
    let mut list = Vec::new();
    let Some(session) = WmiSession::connect() else {
        return list;
    };

    unsafe {
        if let Ok(enumerator) = session.services.ExecQuery(
            &BSTR::from("WQL"),
            &BSTR::from("SELECT InstanceName, ManufacturerName, UserFriendlyName, SerialNumberID FROM WmiMonitorID"),
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        ) {
            loop {
                let mut objs: [Option<IWbemClassObject>; 1] = [None];
                let mut returned = 0u32;
                let hr = enumerator.Next(WBEM_INFINITE, &mut objs, &mut returned);
                if hr.is_err() || returned == 0 {
                    break;
                }
                if let Some(obj) = objs[0].take() {
                    let mut v_inst = VARIANT::default();
                    let mut v_mfr = VARIANT::default();
                    let mut v_name = VARIANT::default();
                    let mut v_ser = VARIANT::default();

                    if obj.Get(w!("InstanceName"), 0, &mut v_inst, None, None).is_ok() {
                        if let Ok(inst_bstr) = BSTR::try_from(&v_inst) {
                            let _ = obj.Get(w!("ManufacturerName"), 0, &mut v_mfr, None, None);
                            let _ = obj.Get(w!("UserFriendlyName"), 0, &mut v_name, None, None);
                            let _ = obj.Get(w!("SerialNumberID"), 0, &mut v_ser, None, None);

                            list.push(WmiMonitorIdentity {
                                instance_name: inst_bstr.to_string(),
                                manufacturer: decode_wmi_u16_array(&v_mfr),
                                model: decode_wmi_u16_array(&v_name),
                                serial: decode_wmi_u16_array(&v_ser),
                            });
                        }
                    }
                }
            }
        }
    }

    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_path_matching_handles_edid_and_wmi_formats() {
        let wmi = "DISPLAY\\AUOD2A2\\5&dcfe8a8&0&UID512_0";
        let edid = "\\\\?\\DISPLAY#AUOD2A2#5&DCFE8A8&0&UID512#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        assert!(matches_device_path(wmi, edid));
        assert!(!matches_device_path(
            wmi,
            "\\\\?\\DISPLAY#AOC2401#5&1111111&0&UID256#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}"
        ));
    }

    #[test]
    fn wmi_query_and_session_drop_do_not_crash() {
        for _ in 0..3 {
            let panels = query_wmi_panels();
            for p in &panels {
                assert!(!p.instance_name.is_empty());
                assert!(p.current <= 100);
            }
            let _ids = query_wmi_identities();
        }
    }
}
