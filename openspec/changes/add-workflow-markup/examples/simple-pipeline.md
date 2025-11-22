# Simple Data Pipeline

This workflow demonstrates a basic data processing pipeline with sequential steps.

## Load Data @data-loader #pipeline
Reads raw data from the input source.
raw_data.csv → loaded_data::DataFrame

```session-toon
execution[3]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 LoadData,data-loader,pipeline,raw_data.csv,loaded_df,1200,success,450
 LoadData,data-loader,pipeline,raw_data_2.csv,loaded_df_2,980,success,420
 LoadData,data-loader,pipeline,raw_data_3.csv,error,450,failed,380
```

---

## Clean Data @data-cleaner #pipeline
Removes duplicates and handles missing values.
loaded_data → cleaned_data::DataFrame

```session-toon
execution[2]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 CleanData,data-cleaner,pipeline,loaded_df,cleaned_df,3400,success,890
 CleanData,data-cleaner,pipeline,loaded_df_2,cleaned_df_2,3100,success,850
```

---

## Transform Data @transformer #pipeline
Applies business logic transformations.
cleaned_data → transformed_data::DataFrame

```session-toon
execution[2]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 TransformData,transformer,pipeline,cleaned_df,transformed_df,2200,success,720
 TransformData,transformer,pipeline,cleaned_df_2,transformed_df_2,2350,success,740
```

---

## Export Results @exporter #pipeline !
Critical: Writes results to database.
transformed_data → success_confirmation::String

```session-toon
execution[2]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 ExportResults,exporter,pipeline,transformed_df,db_confirm_123,850,success,290
 ExportResults,exporter,pipeline,transformed_df_2,db_confirm_124,920,success,310
```

## Summary

This pipeline processed 3 input files with 1 failure in the load phase. Subsequent steps continued with the 2 successfully loaded datasets. Total processing time: ~12 seconds.
