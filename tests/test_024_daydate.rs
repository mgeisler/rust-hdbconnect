mod test_utils;

use chrono::NaiveDate;
use flexi_logger::ReconfigurationHandle;
use hdbconnect::{Connection, HdbResult};
use log::{debug, info, trace};

#[test] // cargo test --test test_024_daydate
pub fn test_024_daydate() -> HdbResult<()> {
    let mut loghandle = test_utils::init_logger();
    let mut connection = test_utils::get_authenticated_connection()?;

    test_daydate(&mut loghandle, &mut connection)?;

    info!("{} calls to DB were executed", connection.get_call_count()?);
    Ok(())
}

// Test the conversion of time values
// - during serialization (input to prepared_statements)
// - during deserialization (result)
fn test_daydate(
    _loghandle: &mut ReconfigurationHandle,
    connection: &mut Connection,
) -> HdbResult<()> {
    info!("verify that NaiveDate values match the expected string representation");

    debug!("prepare the test data");
    let naive_time_values: Vec<NaiveDate> = vec![
        NaiveDate::from_ymd(1, 1, 1),
        NaiveDate::from_ymd(1, 1, 2),
        NaiveDate::from_ymd(2012, 2, 2),
        NaiveDate::from_ymd(2013, 3, 3),
        NaiveDate::from_ymd(2014, 4, 4),
    ];
    let string_values = vec![
        "0001-01-01",
        "0001-01-02",
        "2012-02-02",
        "2013-03-03",
        "2014-04-04",
    ];
    for i in 0..5 {
        assert_eq!(
            naive_time_values[i].format("%Y-%m-%d").to_string(),
            string_values[i]
        );
    }

    // Insert the data such that the conversion "String -> SecondTime" is done on the
    // server side (we assume that this conversion is error-free).
    let insert_stmt = |n, d| {
        format!(
            "insert into TEST_DAYDATE (number,mydate) values({}, '{}')",
            n, d
        )
    };
    connection.multiple_statements_ignore_err(vec!["drop table TEST_DAYDATE"]);
    connection.multiple_statements(vec![
        "create table TEST_DAYDATE (number INT primary key, mydate DAYDATE)",
        &insert_stmt(13, string_values[0]),
        &insert_stmt(14, string_values[1]),
        &insert_stmt(15, string_values[2]),
        &insert_stmt(16, string_values[3]),
        &insert_stmt(17, string_values[4]),
    ])?;

    {
        info!("test the conversion NaiveDate -> DB");
        trace!("calling prepare()");
        let mut prep_stmt = connection
            .prepare("select sum(number) from TEST_DAYDATE where mydate = ? or mydate = ?")?;

        // Enforce that NaiveDate values are converted in the client (with serde) to the DB type:
        trace!("calling add_batch()");
        prep_stmt.add_batch(&(naive_time_values[2], naive_time_values[3]))?;
        trace!("calling execute_batch()");
        let mut response = prep_stmt.execute_batch()?;
        let pd = response.get_parameter_descriptor()?;
        debug!("Parameter Descriptor: {:?}", pd);
        let pd = response.get_parameter_descriptor()?;
        debug!("Parameter Descriptor: {:?}", pd);
        assert!(response.get_parameter_descriptor().is_err());
        let typed_result: i32 = response.into_resultset()?.try_into()?;
        assert_eq!(typed_result, 31);
    }

    {
        info!("test the conversion DB -> NaiveDate");
        let s = "select mydate from TEST_DAYDATE order by number asc";
        let rs = connection.query(s)?;
        trace!("rs = {:?}", rs);
        let times: Vec<NaiveDate> = rs.try_into()?;
        trace!("times = {:?}", times);
        for (time, ntv) in times.iter().zip(naive_time_values.iter()) {
            debug!("{}, {}", time, ntv);
            assert_eq!(time, ntv);
        }
    }

    {
        info!("prove that '' is the same as '0001:01:01'");
        let rows_affected = connection.dml(&insert_stmt(77, ""))?;
        trace!("rows_affected = {}", rows_affected);
        assert_eq!(rows_affected, 1);

        let dates: Vec<NaiveDate> = connection
            .query("select mydate from TEST_DAYDATE where number = 77 or number = 13")?
            .try_into()?;
        trace!("query sent");
        assert_eq!(dates.len(), 2);
        for date in dates {
            assert_eq!(date, naive_time_values[0]);
        }
    }

    {
        info!("test null values");
        let q = "insert into TEST_DAYDATE (number) values(2350)";

        let rows_affected = connection.dml(q)?;
        trace!("rows_affected = {}", rows_affected);
        assert_eq!(rows_affected, 1);

        let date: Option<NaiveDate> = connection
            .query("select mydate from TEST_DAYDATE where number = 2350")?
            .try_into()?;
        trace!("query sent");
        assert_eq!(date, None);
    }

    Ok(())
}
