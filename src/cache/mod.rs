// New cache
// functions get, update, update and get, get and update, get or init
// all should be atomic operation

// For Now we can get file name update the value

// TODO: Need to change it later
// TODO: https://stackoverflow.com/questions/29445026/converting-number-primitives-i32-f64-etc-to-byte-representations
// TODO: Need to use async lock
// TODO: https://docs.rs/async-rwlock/latest/async_rwlock/

lazy_static! {
    static ref LOCK: std::sync::RwLock<u32> = std::sync::RwLock::new(5);
}

pub async fn get(path: &str) -> fpm::Result<usize> {
    match LOCK.try_read() {
        Ok(_) => {
            let value = tokio::fs::read_to_string(path).await?;
            Ok(value.parse()?)
        }
        Err(e) => Err(fpm::Error::GenericError(e.to_string())),
    }
}

pub async fn get_without_lock(path: &str) -> fpm::Result<usize> {
    let value = tokio::fs::read_to_string(path).await?;
    Ok(value.parse()?)
}

pub async fn create(path: &str) -> fpm::Result<usize> {
    use tokio::io::AsyncWriteExt;
    match LOCK.try_write() {
        Ok(_) => {
            let content: usize = 1;
            tokio::fs::File::create(path)
                .await?
                .write_all(content.to_string().as_bytes())
                .await?;
            Ok(get_without_lock(path).await?)
        }
        Err(e) => Err(fpm::Error::GenericError(e.to_string())),
    }
}

pub async fn update_get(path: &str, value: usize) -> fpm::Result<usize> {
    match LOCK.try_write() {
        Ok(_) => {
            let old_value = get_without_lock(path).await?;
            tokio::fs::write(path, (old_value + value).to_string().as_bytes()).await?;
            Ok(get_without_lock(path).await?)
        }
        Err(e) => Err(fpm::Error::GenericError(e.to_string())),
    }
}

pub async fn increment(path: &str) -> fpm::Result<usize> {
    update_get(path, 1).await
}

pub async fn create_or_inc(path: &str) -> fpm::Result<usize> {
    if std::path::Path::new(path).exists() {
        create(path).await
    } else {
        increment(path).await
    }
}

mod tests {}
