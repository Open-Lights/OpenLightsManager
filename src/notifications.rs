use std::time::Duration;
use crate::gui::{Notification, Timer};

pub fn rate_limit_notification() -> Notification {
    Notification {
        title: "Github Rate Limited".to_string(),
        message: "Open Lights Manager has sent too many requests to Github.\nConsider entering a Github Token in Settings to see updates and new apps".to_string(),
        timer: Timer::new(Duration::from_secs(30)),
        id: fastrand::i32(0..i32::MAX),
    }
}

pub fn java_failure_corrupted() -> Notification {
    Notification {
        title: "Java Check Failure".to_string(),
        message: "The provided Java Runtime is either invalid or corrupted.\nTry a different Java Runtime or reinstall the current one.".to_string(),
        timer: Timer::new(Duration::from_secs(15)),
        id: fastrand::i32(0..i32::MAX),
    }
}

pub fn java_failure_issue() -> Notification {
    Notification {
        title: "Java Check Failure".to_string(),
        message: "Failed to run the Java Check.\nPlease try running the Java Check again.\nIf the issue continues, report the issue on Github.".to_string(),
        timer: Timer::new(Duration::from_secs(15)),
        id: fastrand::i32(0..i32::MAX),
    }
}

pub fn java_failure_invalid() -> Notification {
    Notification {
        title: "Java Check Failure".to_string(),
        message: "An invalid Java Runtime has been provided.\nEnsure \"javaw\" or \"java\" has been selected.".to_string(),
        timer: Timer::new(Duration::from_secs(15)),
        id: fastrand::i32(0..i32::MAX),
    }
}

pub fn java_success(stdout: String) -> Notification {
    Notification {
        title: "Java Check Success".to_string(),
        message: format!("{} has been checked.", stdout),
        timer: Timer::new(Duration::from_secs(15)),
        id: fastrand::i32(0..i32::MAX),
    }
}

pub fn app_installation_failure(app: &String) -> Notification {
    Notification {
        title: "App Installation Failure".to_string(),
        message: format!("{} has failed to install.\nEnsure your device is connected to the Internet.", app),
        timer: Timer::new(Duration::from_secs(15)),
        id: fastrand::i32(0..i32::MAX),
    }
}

pub fn app_installation_success(app: &String) -> Notification {
    Notification {
        title: "App Installation Successful".to_string(),
        message: format!("{} has installed.", app),
        timer: Timer::new(Duration::from_secs(15)),
        id: fastrand::i32(0..i32::MAX),
    }
}

pub fn manager_installation_success() -> Notification {
    Notification {
        title: "Open Lights Manager Installation Successful".to_string(),
        message: "Please restart this application for the update to complete.".to_string(),
        timer: Timer::new(Duration::from_secs(15)),
        id: fastrand::i32(0..i32::MAX),
    }
}

pub fn launched_application(name: &String) -> Notification {
    Notification {
        title: format!("{} is launching...", name),
        message: "The application will open momentarily.\nPlease wait.".to_string(),
        timer: Timer::new(Duration::from_secs(10)),
        id: fastrand::i32(0..i32::MAX),
    }
}

pub fn launched_application_missing_java(name: &String) -> Notification {
    Notification {
        title: format!("{} failed to launch", name),
        message: "The application requires a Java Environment.\nPlease install GraalVM from the Browse tab.".to_string(),
        timer: Timer::new(Duration::from_secs(15)),
        id: fastrand::i32(0..i32::MAX),
    }
}