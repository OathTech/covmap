pub const AUDIT_LOG_RETENTION_DAYS: u32 = 90;

pub fn purge_expired_audit_logs(now: Date) {
    for log in store().audit_logs() {
        if log.age_days(now) > AUDIT_LOG_RETENTION_DAYS {
            store().purge(log.id);
        }
    }
}
