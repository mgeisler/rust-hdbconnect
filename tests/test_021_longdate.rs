extern crate chrono;
extern crate flexi_logger;
extern crate hdbconnect;
#[macro_use]
extern crate log;
extern crate serde_json;

mod test_utils;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use flexi_logger::ReconfigurationHandle;
use hdbconnect::HdbResult;

#[test] // cargo test --test test_021_longdate
pub fn test_021_longdate() -> HdbResult<()> {
    let mut loghandle = test_utils::init_logger("info,test_021_longdate=info");

    let count = test_longdate(&mut loghandle)?;
    info!("longdate: {} calls to DB were executed", count);
    Ok(())
}

// Test the conversion of timestamps
// - during serialization (input to prepared_statements)
// - during deserialization (result)
fn test_longdate(_loghandle: &mut ReconfigurationHandle) -> HdbResult<i32> {
    info!("verify that NaiveDateTime values match the expected string representation");

    debug!("prepare the test data");
    let naive_datetime_values: Vec<NaiveDateTime> = vec![
        NaiveDate::from_ymd(1, 1, 1).and_hms_nano(0, 0, 0, 0),
        NaiveDate::from_ymd(1, 1, 1).and_hms_nano(0, 0, 0, 100),
        NaiveDate::from_ymd(2012, 2, 2).and_hms_nano(2, 2, 2, 200_000_000),
        NaiveDate::from_ymd(2013, 3, 3).and_hms_nano(3, 3, 3, 300_000_000),
        NaiveDate::from_ymd(2014, 4, 4).and_hms_nano(4, 4, 4, 400_000_000),
    ];
    let string_values = vec![
        "0001-01-01 00:00:00.000000000",
        "0001-01-01 00:00:00.000000100",
        "2012-02-02 02:02:02.200000000",
        "2013-03-03 03:03:03.300000000",
        "2014-04-04 04:04:04.400000000",
    ];
    for i in 0..5 {
        assert_eq!(
            naive_datetime_values[i]
                .format("%Y-%m-%d %H:%M:%S.%f")
                .to_string(),
            string_values[i]
        );
    }

    let mut connection = test_utils::get_authenticated_connection()?;

    // Insert the data such that the conversion "String -> LongDate" is done on the
    // server side (we assume that this conversion is error-free).
    let insert_stmt = |n, d| {
        format!(
            "insert into TEST_LONGDATE (number,mydate) values({}, '{}')",
            n, d
        )
    };
    connection.multiple_statements_ignore_err(vec!["drop table TEST_LONGDATE"]);
    connection.multiple_statements(vec![
        "create table TEST_LONGDATE (number INT primary key, mydate LONGDATE)",
        &insert_stmt(13, string_values[0]),
        &insert_stmt(14, string_values[1]),
        &insert_stmt(15, string_values[2]),
        &insert_stmt(16, string_values[3]),
        &insert_stmt(17, string_values[4]),
    ])?;

    let mut prepared_stmt = connection
        .prepare("insert into TEST_LONGDATE (number,mydate)  values(?, ?)")
        .unwrap();
    prepared_stmt
        .add_batch(&(&18, &"2018-09-20 17:31:41"))
        .unwrap();
    prepared_stmt.execute_batch().unwrap();

    {
        info!("test the conversion NaiveDateTime -> DB");
        let mut prep_stmt = connection
            .prepare("select sum(number) from TEST_LONGDATE where mydate = ? or mydate = ?")?;
        // Enforce that NaiveDateTime values are converted in the client (with serde) to the DB type:
        prep_stmt.add_batch(&(naive_datetime_values[2], naive_datetime_values[3]))?;
        let mut response = prep_stmt.execute_batch()?;
        debug!(
            "Parameter Descriptor: {:?}",
            response.get_parameter_descriptor()?
        );
        debug!(
            "Parameter Descriptor: {:?}",
            response.get_parameter_descriptor()?
        );
        assert!(response.get_parameter_descriptor().is_err());
        let typed_result: i32 = response.into_resultset()?.try_into()?;
        assert_eq!(typed_result, 31);

        info!("test the conversion DateTime<Utc> -> DB");
        let utc2: DateTime<Utc> = DateTime::from_utc(naive_datetime_values[2], Utc);
        let utc3: DateTime<Utc> = DateTime::from_utc(naive_datetime_values[3], Utc);

        // Enforce that UTC timestamps values are converted here in the client to the DB type:
        prep_stmt.add_batch(&(utc2, utc3))?;
        let typed_result: i32 = prep_stmt.execute_batch()?.into_resultset()?.try_into()?;
        assert_eq!(typed_result, 31_i32);
    }

    {
        info!("test the conversion DB -> NaiveDateTime");
        let s = "select mydate from TEST_LONGDATE order by number asc";
        let rs = connection.query(s)?;
        let dates: Vec<NaiveDateTime> = rs.try_into()?;
        for (date, tvd) in dates.iter().zip(naive_datetime_values.iter()) {
            assert_eq!(date, tvd);
        }
    }

    {
        info!("prove that '' is the same as '0001-01-01 00:00:00.000000000'");
        let rows_affected = connection.dml(&insert_stmt(77, ""))?;
        assert_eq!(rows_affected, 1);
        let dates: Vec<NaiveDateTime> = connection
            .query("select mydate from TEST_LONGDATE where number = 77 or number = 13")?
            .try_into()?;
        assert_eq!(dates.len(), 2);
        for date in dates {
            assert_eq!(date, naive_datetime_values[0]);
        }
    }

    {
        info!("test null values");
        let q = "insert into TEST_LONGDATE (number) values(2350)";

        let rows_affected = connection.dml(&q)?;
        trace!("rows_affected = {}", rows_affected);
        assert_eq!(rows_affected, 1);

        let date: Option<NaiveDateTime> = connection
            .query("select mydate from TEST_LONGDATE where number = 2350")?
            .try_into()?;
        trace!("query sent");
        assert_eq!(date, None);
    }

    Ok(connection.get_call_count()?)
}
