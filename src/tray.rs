use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::rc::Rc;

const SNI_XML: &str = r#"
<node>
  <interface name="org.kde.StatusNotifierItem">
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="WindowId" type="i" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="ItemIsMenu" type="b" access="read"/>
    <method name="ContextMenu">
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
    </method>
    <method name="Activate">
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
    </method>
    <method name="SecondaryActivate">
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
    </method>
    <method name="Scroll">
      <arg name="delta" type="i" direction="in"/>
      <arg name="orientation" type="s" direction="in"/>
    </method>
    <signal name="NewTitle"/>
    <signal name="NewIcon"/>
    <signal name="NewStatus">
      <arg name="status" type="s"/>
    </signal>
  </interface>
</node>
"#;

pub fn setup_tray<F: Fn() + 'static>(toggle_fn: F) {
    let connection = match gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        Ok(c) => c,
        Err(_) => return,
    };

    let node_info = match gio::DBusNodeInfo::for_xml(SNI_XML) {
        Ok(n) => n,
        Err(_) => return,
    };

    let iface_info = match node_info.lookup_interface("org.kde.StatusNotifierItem") {
        Some(i) => i,
        None => return,
    };

    let toggle_rc = Rc::new(toggle_fn);
    let toggle_clone = toggle_rc.clone();

    let reg_result = connection
        .register_object("/StatusNotifierItem", &iface_info)
        .method_call(
            move |_conn, _sender, _obj_path, _iface_name, method_name, _params, invocation| {
                if method_name == "Activate" || method_name == "ContextMenu" {
                    toggle_clone();
                }
                invocation.return_value(None);
            },
        )
        .property(
            move |_conn, _sender, _obj_path, _iface_name, prop_name| match prop_name {
                "Category" => "ApplicationStatus".to_variant(),
                "Id" => "pi-overlay".to_variant(),
                "Title" => "Pi Overlay".to_variant(),
                "Status" => "Active".to_variant(),
                "IconName" => "pi-overlay".to_variant(),
                "ItemIsMenu" => false.to_variant(),
                "WindowId" => 0i32.to_variant(),
                _ => "".to_variant(),
            },
        )
        .build();

    if reg_result.is_err() {
        return;
    }

    // Register with org.kde.StatusNotifierWatcher
    connection.call(
        Some("org.kde.StatusNotifierWatcher"),
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
        "RegisterStatusNotifierItem",
        Some(&(glib::Variant::from("/StatusNotifierItem"),).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        |_| {},
    );
}
