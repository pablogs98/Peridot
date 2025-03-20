drop view revenue0;

create view revenue0 (supplier_no, total_revenue) as
select
    l_suppkey,
    sum(l_extendedprice * (1 - l_discount)) as total_revenue
from
    lineitem
where
    l_shipdate >= cast('1993-01-01' as datetime)
  and l_shipdate < date('1993-01-01', '+3 months')
        group by
        l_suppkey;

select
    s_suppkey,
    s_name,
    s_address,
    s_phone,
    total_revenue
from
    supplier,
    revenue0
where
    s_suppkey = supplier_no
  and total_revenue = (
    select
        max(total_revenue)
    from
        revenue0
)
order by
    s_suppkey;

drop view revenue0;
